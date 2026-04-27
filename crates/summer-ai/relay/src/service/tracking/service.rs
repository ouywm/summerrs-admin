//! `TrackingService` —— 请求完结时一次性落库 3 表：
//!
//! - `ai.request`           —— 客户端视角的一次完整请求（状态、总耗时、首字节延迟）
//! - `ai.request_execution` —— 每次上游 attempt（P9 retry 多次时会有 N 条，attempt_no 1..=N）
//! - `ai.log`               —— 账务 / 审计摘要（token 数 + quota + cost_total）
//!
//! # 关键设计
//!
//! - **两种 emit 入口**：
//!   - [`Self::emit`] —— 单 attempt 场景（流式 / 不走 retry 的调用方），接口不变
//!   - [`Self::emit_with_attempts`] —— P9 retry 的调用方传全量 `Vec<AttemptRecord>`
//! - **共享 worker 池**：走 [`BackgroundTaskQueue`]（4 worker / 4096 容量），避免裸
//!   `tokio::spawn` 在高并发下把 DB 连接池挤爆。
//! - **一次事务**：请求主表 + N 条 execution + log 在一个 `db.begin()` 事务里提交。
//! - **失败只 warn 不传错**：DB 挂了也不影响本次请求响应。
//! - **cost / quota 暂留 0**：本阶段（P5）只落日志不算钱——由 P6 的
//!   `BillingService::settle` 完成后通过 `update_cost_by_request_id` 回填。

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

use super::context::{AttemptRecord, TrackingOutcome};
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
    /// 请求结束时一次性落库（非阻塞）——单 attempt 场景。
    ///
    /// 内部把 `ctx` + `outcome` 合成一条 `AttemptRecord`（attempt_no=1），再走
    /// [`Self::emit_with_attempts`] 统一路径。
    pub fn emit(
        &self,
        ctx: RelayContext,
        outcome: TrackingOutcome,
        request_body: Option<Value>,
        upstream_request_body: Option<Value>,
    ) {
        let attempt = synthesize_single_attempt(&ctx, &outcome, upstream_request_body);
        self.emit_with_attempts(ctx, outcome, vec![attempt], request_body);
    }

    /// 请求结束时一次性落库（非阻塞）——多 attempt（P9 retry）场景。
    ///
    /// - `ctx` 带**最终**选中的 channel / account / upstream_request_id（`attach_channel`
    ///   之后反映最后一次 attempt 的状态）
    /// - `final_outcome` 是最终返给客户端的结果
    /// - `attempts` 按 `attempt_no` 顺序排列；可以为空（表示没到"发上游"这一步，
    ///   比如路由失败 —— execution 表不写任何行）
    /// - `request_body` 是入站客户端 body 的 JSON 快照（脱敏后），落 `ai.request.request_body`
    pub fn emit_with_attempts(
        &self,
        ctx: RelayContext,
        final_outcome: TrackingOutcome,
        attempts: Vec<AttemptRecord>,
        request_body: Option<Value>,
    ) {
        let db = self.db.clone();
        let span = tracing::info_span!(
            "tracking.emit",
            request_id = %ctx.request_id,
            endpoint = %ctx.endpoint,
            attempts = attempts.len(),
        );
        self.bg.spawn(
            async move {
                if let Err(e) = persist(&db, &ctx, &final_outcome, attempts, request_body).await {
                    tracing::warn!(%e, request_id = %ctx.request_id, "tracking persist failed");
                }
            }
            .instrument(span),
        );
    }

    /// 给 billing 结算完成后回填 log.quota / cost_total / price_reference。
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
// 合成兼容单 attempt 的 AttemptRecord（旧 emit 调用路径）
// ---------------------------------------------------------------------------

fn synthesize_single_attempt(
    ctx: &RelayContext,
    outcome: &TrackingOutcome,
    upstream_request_body: Option<Value>,
) -> AttemptRecord {
    let now = chrono::Utc::now().fixed_offset();
    let duration_ms = ctx.elapsed_ms();
    let first_token_ms = ctx.first_token_ms();
    AttemptRecord {
        attempt_no: 1,
        channel_id: ctx.channel_id(),
        account_id: ctx.account_id(),
        request_format: ctx
            .adapter_kind
            .map(|k| k.as_str().to_string())
            .unwrap_or_default(),
        upstream_model: ctx.actual_model.clone().unwrap_or_default(),
        upstream_request_id: ctx
            .upstream_request_id
            .clone()
            .or_else(|| outcome.upstream_request_id().map(str::to_string)),
        sent_headers: ctx
            .sent_headers
            .clone()
            .unwrap_or(Value::Object(Default::default())),
        request_body: upstream_request_body,
        response_body: outcome.response_snapshot(),
        response_status_code: outcome.upstream_status() as i32,
        success: outcome.is_success(),
        error_message: outcome.error_message().to_string(),
        duration_ms,
        first_token_ms,
        started_at: now,
        finished_at: now,
    }
}

// ---------------------------------------------------------------------------
// 内部实现：事务包 (1 request) + (N executions) + (1 log)
// ---------------------------------------------------------------------------

async fn persist(
    db: &DbConn,
    ctx: &RelayContext,
    outcome: &TrackingOutcome,
    attempts: Vec<AttemptRecord>,
    request_body: Option<Value>,
) -> Result<(), sea_orm::DbErr> {
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
        request_headers: Set(ctx.client_headers.clone()),
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

    // ─── 2) ai.request_execution（N 条，按 attempt_no 顺序）─────────
    let mut last_execution_id: i64 = 0;
    for att in &attempts {
        let exec = request_execution::ActiveModel {
            ai_request_id: Set(req_model.id),
            request_id: Set(ctx.request_id.clone()),
            attempt_no: Set(att.attempt_no),
            channel_id: Set(att.channel_id),
            account_id: Set(att.account_id),
            endpoint: Set(ctx.endpoint.to_string()),
            request_format: Set(att.request_format.clone()),
            requested_model: Set(ctx.logical_model.clone()),
            upstream_model: Set(att.upstream_model.clone()),
            upstream_request_id: Set(att.upstream_request_id.clone().unwrap_or_default()),
            request_headers: Set(att.sent_headers.clone()),
            request_body: Set(att
                .request_body
                .clone()
                .unwrap_or(Value::Object(Default::default()))),
            response_body: Set(att.response_body.clone()),
            response_status_code: Set(att.response_status_code),
            status: Set(if att.success {
                request_execution::RequestExecutionStatus::Succeeded
            } else {
                request_execution::RequestExecutionStatus::Failed
            }),
            error_message: Set(att.error_message.clone()),
            duration_ms: Set(att.duration_ms),
            first_token_ms: Set(att.first_token_ms),
            started_at: Set(att.started_at),
            finished_at: Set(Some(att.finished_at)),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        last_execution_id = exec.id;
    }

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
        execution_id: Set(last_execution_id),
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
        quota: Set(0),
        cost_total: Set(BigDecimal::from(0)),
        price_reference: Set(String::new()),
        elapsed_time: Set(duration_ms),
        first_token_time: Set(first_token_ms),
        is_stream: Set(ctx.is_stream),
        request_id: Set(ctx.request_id.clone()),
        dedupe_key: Set(ctx.request_id.clone()),
        upstream_request_id: Set(ctx
            .upstream_request_id
            .clone()
            .or_else(|| outcome.upstream_request_id().map(str::to_string))
            .unwrap_or_default()),
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
            },
            "/v1/chat/completions",
            IngressFormat::OpenAI,
            "gpt-4o-mini",
            false,
            "127.0.0.1",
            "curl/8",
            Value::Object(Default::default()),
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
        assert_eq!(ctx.channel_id(), 0);
        assert_eq!(ctx.account_id(), 0);
    }

    #[test]
    fn synthesize_single_attempt_carries_ctx_and_outcome_fields() {
        let ctx = mk_ctx();
        let out = super::super::context::success(Usage::default(), 200);
        let att = synthesize_single_attempt(&ctx, &out, None);
        assert_eq!(att.attempt_no, 1);
        assert!(att.success);
        assert_eq!(att.response_status_code, 200);
        assert_eq!(att.error_message, "");
        assert_eq!(att.channel_id, 0);
        assert_eq!(att.account_id, 0);
    }

    #[test]
    fn synthesize_single_attempt_failure_propagates_status_and_message() {
        let ctx = mk_ctx();
        let out = super::super::context::failure(503, "upstream responded 503");
        let att = synthesize_single_attempt(&ctx, &out, None);
        assert!(!att.success);
        assert_eq!(att.error_message, "upstream responded 503");
    }
}
