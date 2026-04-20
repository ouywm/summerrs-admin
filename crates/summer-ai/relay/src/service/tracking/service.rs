//! `TrackingService` —— 请求完结时一次性落库 3 表：
//!
//! - `ai.request`           —— 客户端视角的一次完整请求（状态、总耗时、首字节延迟）
//! - `ai.request_execution` —— 每次上游 attempt（v1 固定 attempt_no=1；P9 retry 后可能多次）
//! - `ai.log`               —— 账务 / 审计摘要（token 数 + quota + cost_total）
//!
//! # 关键设计
//!
//! - **共享 worker 池**：handler 在返回前调 [`Self::emit`]，内部交给
//!   [`BackgroundTaskQueue`]（4 worker / 4096 容量）落库。裸 `tokio::spawn`
//!   在高并发下会无限扩张 DB 连接占用，把业务请求挤出连接池；走 BackgroundTaskQueue
//!   可以硬封顶并发上限（见 `crates/summer-plugins/src/background_task`）。
//! - **一次事务**：三个 insert 在 `db.begin()` 的事务里一起提交。
//! - **失败只 warn 不传错**：DB 挂了也不影响本次请求响应；问题交给 metrics / 日志监控。
//! - **关联用 i64 FK**：在事务里先 insert `ai.request` 拿到 id，再用这个 id 填
//!   `ai.request_execution.ai_request_id`。字符串 `request_id` 冗余在三张表里，方便
//!   string-index 跨表查询。
//! - **cost / quota 暂留 0**：本阶段（P5）只落日志不算钱——这两个字段由 P6 的
//!   `BillingService::settle` 返回后再 **UPDATE** 进去（见 `update_cost_by_request_id`）。

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
    prelude::BigDecimal,
};
use serde_json::Value;
use summer::plugin::Service;
use summer_ai_model::entity::requests::{log, request, request_execution};
use summer_plugins::background_task::BackgroundTaskQueue;
use summer_sea_orm::DbConn;
use tracing::Instrument;

use super::context::TrackingOutcome;
use crate::context::RelayContext;

/// 请求追踪落库服务。`#[derive(Service)]` 自动通过 inventory 注册到 component
/// registry；handler 用 `Component<TrackingService>` 取。
#[derive(Clone, Service)]
pub struct TrackingService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    bg: BackgroundTaskQueue,
}

impl TrackingService {
    /// 请求结束时一次性落库（非阻塞）。
    ///
    /// - `ctx` 全部字段已填好（选路 + 发送 + 响应）
    /// - `outcome` 带上成功 / 失败语义
    /// - `request_body` 是客户端原始 body 的 JSON 快照（脱敏后）。传 `None` 只落空对象。
    /// - `upstream_request_body` 是发给上游的 body（给 execution 表）。传 `None` 只落空对象。
    pub fn emit(
        &self,
        ctx: RelayContext,
        outcome: TrackingOutcome,
        request_body: Option<Value>,
        upstream_request_body: Option<Value>,
    ) {
        let db = self.db.clone();
        let span = tracing::info_span!(
            "tracking.emit",
            request_id = %ctx.request_id,
            endpoint = %ctx.endpoint,
        );
        self.bg.spawn(
            async move {
                if let Err(e) =
                    persist(&db, &ctx, &outcome, request_body, upstream_request_body).await
                {
                    tracing::warn!(%e, request_id = %ctx.request_id, "tracking persist failed");
                }
            }
            .instrument(span),
        );
    }

    /// 给 billing 结算完成后回填 log.quota / cost_total / price_reference。
    ///
    /// P6 的 `BillingService::settle` 在扣费完成后调用。分开是因为 settle 走独立的
    /// PG 事务（需要锁 user_quota），和 tracking 一起回滚的话耦合太重。
    pub fn update_cost_by_request_id(
        &self,
        request_id: String,
        quota: i64,
        cost_total: BigDecimal,
        price_reference: String,
    ) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let res = log::Entity::update_many()
                .col_expr(log::Column::Quota, quota.into())
                .col_expr(log::Column::CostTotal, cost_total.clone().into())
                .col_expr(log::Column::PriceReference, price_reference.clone().into())
                .filter(log::Column::RequestId.eq(&request_id))
                .exec(&db)
                .await;
            if let Err(e) = res {
                tracing::warn!(%e, %request_id, "tracking update_cost failed");
            }
        });
    }
}

// ---------------------------------------------------------------------------
// 内部实现：事务包三个 insert
// ---------------------------------------------------------------------------

async fn persist(
    db: &DbConn,
    ctx: &RelayContext,
    outcome: &TrackingOutcome,
    request_body: Option<Value>,
    upstream_request_body: Option<Value>,
) -> Result<(), sea_orm::DbErr> {
    let now = chrono::Utc::now().fixed_offset();
    let duration_ms = ctx.elapsed_ms();
    let first_token_ms = ctx.first_token_ms();
    let usage = outcome.usage();

    let txn = db.begin().await?;

    // ─── 1) ai.request ────────────────────────────────────────────
    let req_model = request::ActiveModel {
        request_id: Set(ctx.request_id.clone()),
        user_id: Set(ctx.token.user_id),
        token_id: Set(ctx.token.token_id),
        project_id: Set(ctx.token.project_id),
        conversation_id: Set(0),
        message_id: Set(0),
        session_id: Set(0),
        thread_id: Set(0),
        trace_id: Set(0),
        channel_group: Set(ctx
            .channel
            .as_ref()
            .map(|c| c.channel_group.clone())
            .unwrap_or_default()),
        source_type: Set("api".to_string()),
        endpoint: Set(ctx.endpoint.to_string()),
        request_format: Set(ctx.format.as_str().to_string()),
        requested_model: Set(ctx.logical_model.clone()),
        upstream_model: Set(ctx.actual_model.clone().unwrap_or_default()),
        is_stream: Set(ctx.is_stream),
        client_ip: Set(ctx.client_ip.clone()),
        user_agent: Set(ctx.user_agent.clone()),
        request_headers: Set(Value::Object(Default::default())),
        request_body: Set(request_body
            .clone()
            .unwrap_or(Value::Object(Default::default()))),
        response_body: Set(outcome.response_snapshot()),
        response_status_code: Set(outcome.client_status() as i32),
        status: Set(if outcome.is_success() {
            request::RequestStatus::Succeeded
        } else {
            request::RequestStatus::Failed
        }),
        error_message: Set(outcome.error_message().to_string()),
        duration_ms: Set(duration_ms),
        first_token_ms: Set(first_token_ms),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    // ─── 2) ai.request_execution ──────────────────────────────────
    let exec = request_execution::ActiveModel {
        ai_request_id: Set(req_model.id),
        request_id: Set(ctx.request_id.clone()),
        attempt_no: Set(1),
        channel_id: Set(ctx.channel_id()),
        account_id: Set(ctx.account_id()),
        endpoint: Set(ctx.endpoint.to_string()),
        request_format: Set(ctx
            .adapter_kind
            .map(|k| k.as_str().to_string())
            .unwrap_or_default()),
        requested_model: Set(ctx.logical_model.clone()),
        upstream_model: Set(ctx.actual_model.clone().unwrap_or_default()),
        upstream_request_id: Set(outcome.upstream_request_id().unwrap_or("").to_string()),
        request_headers: Set(Value::Object(Default::default())),
        request_body: Set(upstream_request_body.unwrap_or(Value::Object(Default::default()))),
        response_body: Set(outcome.response_snapshot()),
        response_status_code: Set(outcome.upstream_status() as i32),
        status: Set(if outcome.is_success() {
            request_execution::RequestExecutionStatus::Succeeded
        } else {
            request_execution::RequestExecutionStatus::Failed
        }),
        error_message: Set(outcome.error_message().to_string()),
        duration_ms: Set(duration_ms),
        first_token_ms: Set(first_token_ms),
        started_at: Set(now),
        finished_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    // ─── 3) ai.log ────────────────────────────────────────────────
    let (prompt_tokens, completion_tokens, total_tokens, cached_tokens, reasoning_tokens) = (
        usage.prompt_tokens as i32,
        usage.completion_tokens as i32,
        usage.total_tokens as i32,
        usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0) as i32,
        usage
            .completion_tokens_details
            .as_ref()
            .and_then(|d| d.reasoning_tokens)
            .unwrap_or(0) as i32,
    );

    log::ActiveModel {
        user_id: Set(ctx.token.user_id),
        token_id: Set(ctx.token.token_id),
        token_name: Set(ctx.token.token_name.clone()),
        project_id: Set(ctx.token.project_id),
        conversation_id: Set(0),
        message_id: Set(0),
        session_id: Set(0),
        thread_id: Set(0),
        trace_id: Set(0),
        channel_id: Set(ctx.channel_id()),
        channel_name: Set(ctx
            .channel
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()),
        account_id: Set(ctx.account_id()),
        account_name: Set(ctx
            .account
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default()),
        execution_id: Set(exec.id),
        endpoint: Set(ctx.endpoint.to_string()),
        request_format: Set(ctx.format.as_str().to_string()),
        requested_model: Set(ctx.logical_model.clone()),
        upstream_model: Set(ctx.actual_model.clone().unwrap_or_default()),
        model_name: Set(ctx
            .actual_model
            .clone()
            .unwrap_or_else(|| ctx.logical_model.clone())),
        prompt_tokens: Set(prompt_tokens),
        completion_tokens: Set(completion_tokens),
        total_tokens: Set(total_tokens),
        cached_tokens: Set(cached_tokens),
        reasoning_tokens: Set(reasoning_tokens),
        quota: Set(0), // P6 settle 后 UPDATE 回来
        cost_total: Set(BigDecimal::from(0)),
        price_reference: Set(String::new()),
        elapsed_time: Set(duration_ms),
        first_token_time: Set(first_token_ms),
        is_stream: Set(ctx.is_stream),
        request_id: Set(ctx.request_id.clone()),
        dedupe_key: Set(ctx.request_id.clone()), // v1 直接复用 request_id
        upstream_request_id: Set(outcome.upstream_request_id().unwrap_or("").to_string()),
        status_code: Set(outcome.client_status() as i32),
        client_ip: Set(ctx.client_ip.clone()),
        user_agent: Set(ctx.user_agent.clone()),
        content: Set(outcome.error_message().to_string()),
        log_type: Set(log::LogType::Consumption),
        status: Set(if outcome.is_success() {
            log::LogStatus::Succeeded
        } else {
            log::LogStatus::Failed
        }),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::RelayContext;
    use crate::convert::ingress::IngressFormat;
    use summer_ai_core::Usage;

    fn mk_ctx() -> RelayContext {
        use crate::auth::AiTokenContext;
        RelayContext::begin(
            AiTokenContext {
                token_id: 1,
                user_id: 2,
                project_id: 3,
                service_account_id: 0,
                token_name: "dev".into(),
                key_prefix: "sk".into(),
                unlimited_quota: true,
                remain_quota: 100,
                group_code_override: String::new(),
                allowed_models: Vec::new(),
                allowed_endpoint_scopes: Vec::new(),
            },
            "/v1/chat/completions",
            IngressFormat::OpenAI,
            "gpt-4o-mini",
            false,
            "127.0.0.1",
            "curl/8",
        )
    }

    #[test]
    fn outcome_success_exposes_upstream_status() {
        let o = super::super::context::success(Usage::default(), 200);
        assert!(o.is_success());
        assert_eq!(o.client_status(), 200);
        assert_eq!(o.upstream_status(), 200);
    }

    #[test]
    fn outcome_failure_contains_message() {
        let o = super::super::context::failure(502, "upstream 502");
        assert!(!o.is_success());
        assert_eq!(o.client_status(), 502);
        assert_eq!(o.error_message(), "upstream 502");
        assert_eq!(o.upstream_status(), 0);
    }

    #[test]
    fn relay_ctx_fields_flow_through_usage_extraction() {
        let ctx = mk_ctx();
        assert_eq!(ctx.token.user_id, 2);
        assert_eq!(ctx.token.token_id, 1);
        // channel/account 未 attach 时为 0
        assert_eq!(ctx.channel_id(), 0);
        assert_eq!(ctx.account_id(), 0);
    }
}
