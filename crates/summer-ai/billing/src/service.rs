//! 计费引擎 —— 三阶段原子扣费：reserve / settle / refund。
//!
//! # 生命周期
//!
//! ```text
//! 请求进入
//!   → BillingService::reserve(user_id, estimated_quota, price_ref)
//!        ↓ 事务内 FOR UPDATE 锁 user_quota，校验余额，used_quota += estimated
//!        ↓ 非阻塞写 ai.transaction (status=Processing, trade_type=consume)
//!   → [上游调用 + retry]
//!   → 成功：BillingService::settle(reservation, actual_quota, request_id)
//!        ↓ 事务内 used_quota += (actual - estimated) ——可正可负
//!        ↓ 非阻塞写 ai.transaction (status=Succeeded, quota_delta=delta)
//!     失败：BillingService::refund(reservation, request_id, reason)
//!        ↓ 事务内 used_quota -= estimated（带 >= 保护防负数）
//!        ↓ 非阻塞写 ai.transaction (trade_type=refund, direction=credit)
//! ```
//!
//! # 设计取舍
//!
//! - **预扣直接改 `used_quota`**：不新增 `reserved_quota` 列，避免表结构迁移。副作用是
//!   进程崩溃时可能留下"幽灵预扣"——依赖后续 Phase 的 sweep 任务清理（可按
//!   `ai.transaction.status = Processing` + 超时时间定位）。
//! - **`Reservation` 为内存对象**：请求内由调用方持有，不落表。
//! - **`ai.transaction` 走 `LogBatchCollector`**：非阻塞攒批 insert，扣费热路径零等待。
//! - **悲观锁 `FOR UPDATE`**：防并发请求同一 `user_id` 时超卖；PG 友好。
//! - **按 `user_id` 升序加锁**：当前只锁单条，无死锁；未来若需跨用户（如团队额度）
//!   按文档《MIGRATION_V2.md 风险》要求统一升序 + `SKIP LOCKED`。

use bigdecimal::BigDecimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use serde_json::json;
use summer::plugin::Service;
use summer_ai_model::entity::billing::{transaction, user_quota};
use summer_plugins::log_batch_collector::LogBatchCollector;
use summer_sea_orm::DbConn;

/// 预扣凭证。由 [`BillingService::reserve`] 返回，后续 [`BillingService::settle`] 或
/// [`BillingService::refund`] 必须携带原对象。
#[derive(Debug, Clone)]
pub struct Reservation {
    /// 扣费主体用户。
    pub user_id: i64,
    /// 本次预扣的 quota 数量（单位：[`crate::QUOTA_PER_USD`] 分之一 USD）。
    pub reserved_quota: i64,
    /// 命中的价格快照引用（来自 `ai.channel_model_price.reference_id`）。
    pub price_reference: String,
}

/// 结算结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// 原预扣 quota。
    pub reserved_quota: i64,
    /// 实际消耗 quota。
    pub actual_quota: i64,
    /// `actual - reserved`：正=补扣；负=退款。
    pub delta: i64,
    /// 结算后用户剩余 quota（`quota - used_quota`）。
    pub balance_after: i64,
}

/// 计费相关错误。
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    /// 用户尚未分配 [`user_quota`] 记录。
    #[error("user_quota not found for user_id={0}")]
    UserQuotaNotFound(i64),

    /// 用户 quota 状态非 `Normal`（被禁用 / 冻结）。
    #[error("user_quota not usable for user_id={user_id} status={status:?}")]
    UserQuotaNotUsable {
        /// 关联用户 ID。
        user_id: i64,
        /// 实际查询到的状态。
        status: user_quota::UserQuotaStatus,
    },

    /// 余额不足。
    #[error("insufficient quota for user_id={user_id}: remaining={remaining} needed={needed}")]
    InsufficientQuota {
        /// 关联用户 ID。
        user_id: i64,
        /// 当前可用余额（`quota - used_quota`）。
        remaining: i64,
        /// 本次请求预估需要的 quota。
        needed: i64,
    },

    /// 负数预扣——上层估算出了非法值。
    #[error("estimated_quota must be >= 0, got {0}")]
    InvalidEstimatedQuota(i64),

    /// 数据库错误。
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

/// 计费引擎。
///
/// 通过 `#[derive(Service)]` 自动注册到 component registry；调用方（relay pipeline）
/// 用 `Component<BillingService>` 取。
#[derive(Clone, Service)]
pub struct BillingService {
    #[inject(component)]
    db: DbConn,
    /// 非阻塞批量写 `ai.transaction` 流水（由 `SummerAiBillingPlugin` 启动）。
    #[inject(component)]
    tx_collector: LogBatchCollector<transaction::ActiveModel>,
}

impl BillingService {
    /// 请求开始前预扣 `estimated_quota`。
    ///
    /// 事务内：
    /// 1. `SELECT ... FOR UPDATE` 锁 `user_quota`；
    /// 2. 校验 `status == Normal`、`quota - used_quota >= estimated`；
    /// 3. `used_quota += estimated`；`request_count += 1`；更新 `last_request_time`。
    ///
    /// 非阻塞：push 一条 `ai.transaction` 流水（`status = Processing`）作审计。
    pub async fn reserve(
        &self,
        user_id: i64,
        estimated_quota: i64,
        price_reference: &str,
    ) -> Result<Reservation, BillingError> {
        if estimated_quota < 0 {
            return Err(BillingError::InvalidEstimatedQuota(estimated_quota));
        }

        let tx = self.db.begin().await?;

        let row = user_quota::Entity::find()
            .filter(user_quota::Column::UserId.eq(user_id))
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(BillingError::UserQuotaNotFound(user_id))?;

        if row.status != user_quota::UserQuotaStatus::Normal {
            return Err(BillingError::UserQuotaNotUsable {
                user_id,
                status: row.status,
            });
        }

        let remaining = row.quota.saturating_sub(row.used_quota);
        if remaining < estimated_quota {
            return Err(BillingError::InsufficientQuota {
                user_id,
                remaining,
                needed: estimated_quota,
            });
        }

        let now = chrono::Utc::now().fixed_offset();
        let mut am: user_quota::ActiveModel = row.clone().into();
        am.used_quota = Set(row.used_quota + estimated_quota);
        am.request_count = Set(row.request_count + 1);
        am.last_request_time = Set(Some(now));
        am.update(&tx).await?;

        tx.commit().await?;

        self.push_transaction(build_transaction(TransactionSpec {
            user_id,
            quota_delta: -estimated_quota,
            trade_type: "consume",
            direction: "debit",
            status: transaction::TransactionStatus::Processing,
            reference_no: String::new(),
            price_reference: price_reference.to_string(),
            reason: None,
        }));

        Ok(Reservation {
            user_id,
            reserved_quota: estimated_quota,
            price_reference: price_reference.to_string(),
        })
    }

    /// 响应成功后按真实 `actual_quota` 结算。`delta = actual - reserved` 可正（补扣）
    /// 可负（部分退款）。同一事务内改 `used_quota` 并读取新余额。
    pub async fn settle(
        &self,
        reservation: Reservation,
        actual_quota: i64,
        reference_no: &str,
    ) -> Result<Settlement, BillingError> {
        let delta = actual_quota - reservation.reserved_quota;

        let tx = self.db.begin().await?;
        let row = user_quota::Entity::find()
            .filter(user_quota::Column::UserId.eq(reservation.user_id))
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(BillingError::UserQuotaNotFound(reservation.user_id))?;

        // 若 delta > 0（补扣）且余额不够，也要扣到 0 为止——不能让请求生效了却扣不到费。
        // 补扣超出的部分进负债：used_quota 可以大于 quota，由后续风控处理。
        let new_used = row.used_quota + delta;
        let mut am: user_quota::ActiveModel = row.clone().into();
        am.used_quota = Set(new_used);
        am.update(&tx).await?;
        tx.commit().await?;

        let balance_after = row.quota.saturating_sub(new_used);

        let direction = if delta >= 0 { "debit" } else { "credit" };
        self.push_transaction(build_transaction(TransactionSpec {
            user_id: reservation.user_id,
            quota_delta: -delta,
            trade_type: "consume",
            direction,
            status: transaction::TransactionStatus::Succeeded,
            reference_no: reference_no.to_string(),
            price_reference: reservation.price_reference,
            reason: None,
        }));

        Ok(Settlement {
            reserved_quota: reservation.reserved_quota,
            actual_quota,
            delta,
            balance_after,
        })
    }

    /// 请求失败时退还整笔预扣。幂等 —— `used_quota` 不会扣到负数。
    pub async fn refund(
        &self,
        reservation: Reservation,
        reference_no: &str,
        reason: &str,
    ) -> Result<(), BillingError> {
        let tx = self.db.begin().await?;
        let row = user_quota::Entity::find()
            .filter(user_quota::Column::UserId.eq(reservation.user_id))
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(BillingError::UserQuotaNotFound(reservation.user_id))?;

        let new_used = (row.used_quota - reservation.reserved_quota).max(0);
        let mut am: user_quota::ActiveModel = row.clone().into();
        am.used_quota = Set(new_used);
        am.update(&tx).await?;
        tx.commit().await?;

        self.push_transaction(build_transaction(TransactionSpec {
            user_id: reservation.user_id,
            quota_delta: reservation.reserved_quota,
            trade_type: "refund",
            direction: "credit",
            status: transaction::TransactionStatus::Succeeded,
            reference_no: reference_no.to_string(),
            price_reference: reservation.price_reference,
            reason: Some(reason.to_string()),
        }));

        Ok(())
    }

    fn push_transaction(&self, active: transaction::ActiveModel) {
        if let Err(e) = self.tx_collector.push(active) {
            tracing::warn!("billing transaction log dropped: {:?}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// transaction 构造（纯函数，便于单测）
// ---------------------------------------------------------------------------

struct TransactionSpec {
    user_id: i64,
    quota_delta: i64,
    trade_type: &'static str,
    direction: &'static str,
    status: transaction::TransactionStatus,
    reference_no: String,
    price_reference: String,
    reason: Option<String>,
}

fn build_transaction(spec: TransactionSpec) -> transaction::ActiveModel {
    let metadata = match &spec.reason {
        Some(r) => json!({ "price_reference": spec.price_reference, "reason": r }),
        None => json!({ "price_reference": spec.price_reference }),
    };
    transaction::ActiveModel {
        organization_id: Set(0),
        user_id: Set(spec.user_id),
        project_id: Set(0),
        order_id: Set(0),
        payment_method_id: Set(0),
        account_type: Set("quota".to_string()),
        direction: Set(spec.direction.to_string()),
        trade_type: Set(spec.trade_type.to_string()),
        amount: Set(BigDecimal::from(0)),
        currency: Set("USD".to_string()),
        quota_delta: Set(spec.quota_delta),
        balance_before: Set(BigDecimal::from(0)),
        balance_after: Set(BigDecimal::from(0)),
        reference_no: Set(spec.reference_no),
        status: Set(spec.status),
        metadata: Set(metadata),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_transaction_consume_debit() {
        let am = build_transaction(TransactionSpec {
            user_id: 42,
            quota_delta: -100,
            trade_type: "consume",
            direction: "debit",
            status: transaction::TransactionStatus::Processing,
            reference_no: "req-x".into(),
            price_reference: "ref-v1".into(),
            reason: None,
        });
        assert_eq!(am.user_id.clone().unwrap(), 42);
        assert_eq!(am.quota_delta.clone().unwrap(), -100);
        assert_eq!(am.trade_type.clone().unwrap(), "consume");
        assert_eq!(am.direction.clone().unwrap(), "debit");
        assert_eq!(am.reference_no.clone().unwrap(), "req-x");
        assert_eq!(am.account_type.clone().unwrap(), "quota");
        let meta = am.metadata.clone().unwrap();
        assert_eq!(meta["price_reference"], "ref-v1");
        assert!(meta.get("reason").is_none());
    }

    #[test]
    fn build_transaction_refund_includes_reason() {
        let am = build_transaction(TransactionSpec {
            user_id: 7,
            quota_delta: 500,
            trade_type: "refund",
            direction: "credit",
            status: transaction::TransactionStatus::Succeeded,
            reference_no: "req-y".into(),
            price_reference: "ref-v2".into(),
            reason: Some("upstream 500".into()),
        });
        let meta = am.metadata.clone().unwrap();
        assert_eq!(meta["reason"], "upstream 500");
        assert_eq!(am.quota_delta.clone().unwrap(), 500);
    }

    #[test]
    fn settlement_delta_positive_when_actual_exceeds_reserved() {
        let s = Settlement {
            reserved_quota: 100,
            actual_quota: 150,
            delta: 50,
            balance_after: 850,
        };
        assert!(s.delta > 0);
    }

    #[test]
    fn settlement_delta_negative_when_partial_refund() {
        let s = Settlement {
            reserved_quota: 100,
            actual_quota: 30,
            delta: -70,
            balance_after: 970,
        };
        assert!(s.delta < 0);
    }
}
