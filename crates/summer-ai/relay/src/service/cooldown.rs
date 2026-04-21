//! `CooldownService` —— 上游 account / key 级失败后的 DB 冷却写入（P9 S4）。
//!
//! retry loop 在 [`crate::pipeline`] 的失败分支调用本服务。三类错误分别写不同字段：
//!
//! | 上游状态 | 字段 | 语义 |
//! |---|---|---|
//! | 429            | `channel_account.rate_limited_until = now + secs` | 限流冷却窗口 |
//! | 503 / 529      | `channel_account.overload_until = now + secs`     | 过载冷却窗口 |
//! | 401 / 403      | `channel_account.disabled_api_keys[]` append `{key,disabled_at,error_code,reason}` | 坏 key 拉黑 |
//!
//! [`ChannelStore::candidates`] 已经按 `rate_limited_until > now`、`overload_until > now`
//! 过滤；[`crate::service::key_picker`] 基于 `enabled_api_keys()` 自动跳过 `disabled_api_keys`。
//! 本服务**只负责写入**，激活那两处过滤死逻辑。
//!
//! # 非阻塞模型
//!
//! - 入口方法 `&self`，非 async，立刻返回。
//! - 实际 DB 操作通过 [`BackgroundTaskQueue::spawn`] 丢给共享 worker 池。
//! - 失败只 `tracing::warn!`——冷却写入失败不该阻断 retry / 影响请求响应。
//!
//! # 并发安全
//!
//! `disable_key` 先 SELECT JSONB，在内存里去重合并，再 UPDATE。两个失败同时写入
//! 同一 account 有极小概率"后写覆盖前写"导致漏一条 entry，但下次触发时还会补上，
//! 最终一致即可——不为此加 `SELECT FOR UPDATE`。
//!
//! [`ChannelStore::candidates`]: crate::service::channel_store::ChannelStore::candidates

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, prelude::*,
};
use serde_json::{Value, json};
use summer::plugin::Service;
use summer_ai_model::entity::channels::channel_account;
use summer_plugins::background_task::BackgroundTaskQueue;
use summer_sea_orm::DbConn;
use tracing::Instrument;

#[derive(Clone, Service)]
pub struct CooldownService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    bg: BackgroundTaskQueue,
}

impl CooldownService {
    /// 上游返 429 → 写 `rate_limited_until = now + secs`。
    pub fn mark_rate_limited(&self, account_id: i64, secs: i64, error_message: impl Into<String>) {
        let db = self.db.clone();
        let msg = error_message.into();
        let span = tracing::info_span!("cooldown.mark_rate_limited", account_id, secs,);
        self.bg.spawn(
            async move {
                let until = Utc::now().fixed_offset() + chrono::Duration::seconds(secs);
                let res = channel_account::Entity::update_many()
                    .col_expr(
                        channel_account::Column::RateLimitedUntil,
                        Expr::value(Some(until)),
                    )
                    .col_expr(
                        channel_account::Column::LastErrorAt,
                        Expr::value(Some(Utc::now().fixed_offset())),
                    )
                    .col_expr(
                        channel_account::Column::LastErrorCode,
                        Expr::value("429".to_string()),
                    )
                    .col_expr(channel_account::Column::LastErrorMessage, Expr::value(msg))
                    .filter(channel_account::Column::Id.eq(account_id))
                    .exec(&db)
                    .await;
                if let Err(e) = res {
                    tracing::warn!(%e, account_id, "cooldown mark_rate_limited failed");
                }
            }
            .instrument(span),
        );
    }

    /// 上游返 503 / 529 → 写 `overload_until = now + secs`。
    pub fn mark_overloaded(
        &self,
        account_id: i64,
        secs: i64,
        upstream_status: u16,
        error_message: impl Into<String>,
    ) {
        let db = self.db.clone();
        let msg = error_message.into();
        let code = upstream_status.to_string();
        let span = tracing::info_span!(
            "cooldown.mark_overloaded",
            account_id,
            secs,
            upstream_status,
        );
        self.bg.spawn(
            async move {
                let until = Utc::now().fixed_offset() + chrono::Duration::seconds(secs);
                let res = channel_account::Entity::update_many()
                    .col_expr(
                        channel_account::Column::OverloadUntil,
                        Expr::value(Some(until)),
                    )
                    .col_expr(
                        channel_account::Column::LastErrorAt,
                        Expr::value(Some(Utc::now().fixed_offset())),
                    )
                    .col_expr(channel_account::Column::LastErrorCode, Expr::value(code))
                    .col_expr(channel_account::Column::LastErrorMessage, Expr::value(msg))
                    .filter(channel_account::Column::Id.eq(account_id))
                    .exec(&db)
                    .await;
                if let Err(e) = res {
                    tracing::warn!(%e, account_id, "cooldown mark_overloaded failed");
                }
            }
            .instrument(span),
        );
    }

    /// 上游返 401 / 403 → 把 `key` append 进 `disabled_api_keys` JSONB 数组。
    ///
    /// 先 SELECT 出当前值，[`merge_disabled_key`] 去重 + append，再 UPDATE。
    pub fn disable_key(
        &self,
        account_id: i64,
        key: impl Into<String>,
        upstream_status: u16,
        reason: impl Into<String>,
    ) {
        let db = self.db.clone();
        let key_str = key.into();
        let reason_str = reason.into();
        let span = tracing::info_span!("cooldown.disable_key", account_id, upstream_status,);
        self.bg.spawn(
            async move {
                if let Err(e) =
                    disable_key_impl(&db, account_id, &key_str, upstream_status, &reason_str).await
                {
                    tracing::warn!(%e, account_id, "cooldown disable_key failed");
                }
            }
            .instrument(span),
        );
    }
}

async fn disable_key_impl(
    db: &DbConn,
    account_id: i64,
    key: &str,
    upstream_status: u16,
    reason: &str,
) -> Result<(), DbErr> {
    let Some(row) = channel_account::Entity::find_by_id(account_id)
        .one(db)
        .await?
    else {
        return Err(DbErr::RecordNotFound(format!(
            "channel_account id={account_id}"
        )));
    };

    let new_entry = json!({
        "key": key,
        "disabled_at": Utc::now().fixed_offset().to_rfc3339(),
        "error_code": upstream_status,
        "reason": reason,
    });
    let merged = merge_disabled_key(&row.disabled_api_keys, key, new_entry);
    if merged == row.disabled_api_keys {
        // 已经拉黑过，跳过 UPDATE。
        return Ok(());
    }

    let mut am: channel_account::ActiveModel = row.into();
    am.disabled_api_keys = Set(merged);
    am.last_error_at = Set(Some(Utc::now().fixed_offset()));
    am.last_error_code = Set(upstream_status.to_string());
    am.last_error_message = Set(reason.to_string());
    am.update(db).await?;
    Ok(())
}

/// 把 `new_entry` append 进 `existing` JSONB 数组——若已有相同 `key` 的条目则返回原值。
///
/// - `existing` 不是数组 / 缺失 → 当空数组处理
/// - 幂等：重复拉黑同一 key 返回原 `existing`（调用方可据此跳过 UPDATE）
fn merge_disabled_key(existing: &Value, key: &str, new_entry: Value) -> Value {
    let mut arr = existing.as_array().cloned().unwrap_or_default();
    let already = arr
        .iter()
        .any(|item| item.get("key").and_then(|v| v.as_str()) == Some(key));
    if already {
        return existing.clone();
    }
    arr.push(new_entry);
    Value::Array(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_into_empty_array_appends_entry() {
        let existing = json!([]);
        let entry = json!({"key": "sk-1", "error_code": 401});
        let merged = merge_disabled_key(&existing, "sk-1", entry);
        let arr = merged.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("key").unwrap(), "sk-1");
    }

    #[test]
    fn merge_into_null_treats_as_empty() {
        let existing = Value::Null;
        let entry = json!({"key": "sk-2", "error_code": 403});
        let merged = merge_disabled_key(&existing, "sk-2", entry);
        assert_eq!(merged.as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_with_duplicate_key_returns_unchanged() {
        let existing = json!([
            {"key": "sk-1", "disabled_at": "2026-01-01T00:00:00+00:00", "error_code": 401, "reason": "old"}
        ]);
        let entry = json!({"key": "sk-1", "error_code": 403, "reason": "new"});
        let merged = merge_disabled_key(&existing, "sk-1", entry);
        assert_eq!(merged, existing);
    }

    #[test]
    fn merge_different_key_appends_preserving_existing() {
        let existing = json!([
            {"key": "sk-1", "error_code": 401}
        ]);
        let entry = json!({"key": "sk-2", "error_code": 403});
        let merged = merge_disabled_key(&existing, "sk-2", entry);
        let arr = merged.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("key").unwrap(), "sk-1");
        assert_eq!(arr[1].get("key").unwrap(), "sk-2");
    }

    #[test]
    fn merge_with_non_array_existing_falls_back_to_fresh() {
        let existing = json!({"not": "an array"});
        let entry = json!({"key": "sk-x", "error_code": 401});
        let merged = merge_disabled_key(&existing, "sk-x", entry);
        assert_eq!(merged.as_array().unwrap().len(), 1);
    }
}
