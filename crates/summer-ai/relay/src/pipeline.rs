//! `PipelineCall` —— 四入口 handler 的统一执行引擎。
//!
//! 一次请求从 "收到客户端 wire" 到 "返响应给客户端 + 落 tracking" 的完整编排。
//! 四个 handler（openai chat / openai responses / claude messages / gemini generate_content）
//! 共用这个引擎，handler 自身只负责**两件事**：
//!
//! 1. 从 HTTP extractor 构造 `PipelineCall<I>`
//! 2. 按 `EngineOutcome<I>` 的两个分支封成各自协议的 `Response`
//!
//! # 流程
//!
//! ```text
//!   RelayContext::begin
//!       │
//!       ▼
//!   ChannelStore::candidates ──(空)──► NoAvailableChannel → tracking.emit_with_attempts
//!       │
//!       ▼
//!   IngressConverter::to_canonical（用第一个候选的 kind / actual_model）
//!       │
//!       ▼
//!   billing prepare：resolve 价格 + group_ratio → estimate_quota → reserve
//!       │  （unlimited_quota 用户跳过整段 billing）
//!       │
//!       ├─── is_stream=false ───► for candidate in candidates:
//!       │                              attach_channel + invoke_non_stream
//!       │                              ├── Ok → settle(actual) → push AttemptRecord(success) → return
//!       │                              └── Err → push AttemptRecord(failure)
//!       │                                   ├── retry_kind=Fatal → break → refund
//!       │                                   └── 否则 continue 下一个候选
//!       │                         循环结束还没 return → refund → emit_with_attempts(最后 err)
//!       │
//!       └─── is_stream=true ────► invoke_stream_raw（单 attempt，保留原 tracking 路径）
//!                                     │
//!                                     └── oneshot + tokio::spawn(settle/refund + tracking.emit)
//! ```
//!
//! # 不做什么
//!
//! - **流式不做** retry（stream 一旦开始输出再切 channel 无法复原客户端侧已发的字节）
//! - **不做** egress format 最后包装（`sse_response` / `Json` 由 handler 自己做——
//!   因为 Gemini `streamGenerateContent` 有 SSE vs JSON-array 两种呈现模式）
//!
//! # Billing 设计取舍
//!
//! - **reserve 的估算价用第一个候选的渠道价 + token 所属 group_ratio**：retry 换渠道
//!   时价差由 settle 的 `delta` 吸收。
//! - **settle 的实际价**：仍用 reserve 时的 PriceTable（第一个候选 + ratio）。不同渠道
//!   的单价差异进入 delta 计算。后续 Phase 若需精确，可在 settle 前重新按命中渠道
//!   resolve 一次。
//! - **unlimited_quota 用户跳过整个 billing**：代表内部 / 开发 token，不计费不扣额。

use std::pin::Pin;
use std::time::Instant;

use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use tokio::sync::oneshot;

use summer_ai_billing::{
    BillingError, BillingService, CostBreakdown, PriceResolver, PriceTable, Reservation,
    compute_cost, estimate_quota,
};
use summer_ai_core::{AdapterDispatcher, EndpointScope};

use crate::auth::AiTokenContext;
use crate::context::{ClientRequestMeta, RelayContext};
use crate::convert::ingress::{IngressConverter, IngressCtx, IngressFormat};
use crate::error::{RelayError, RelayResult, RetryKind};
use crate::service::channel_store::{Candidate, ChannelStore};
use crate::service::chat;
use crate::service::cooldown::CooldownService;
use crate::service::stream_driver::{self, StreamOutcome};
use crate::service::tracking::{AttemptRecord, TrackingOutcome, TrackingService};

/// 一次请求成功进入 billing 路径时携带的状态。
///
/// `None` 表示跳过计费（`AiTokenContext::unlimited_quota = true` 的内部 token）。
struct BillingGuard {
    billing: BillingService,
    reservation: Reservation,
    /// reserve 时确定的、已应用 group_ratio 的价格表；settle 阶段拿实际 usage 回用。
    price: PriceTable,
}

/// 一次入口请求的所有上下文参数。
pub struct PipelineCall<I: IngressConverter> {
    /// 请求 ID（由根路由 request-id 中间件注入），用于 tracking / billing 关联。
    pub request_id: String,
    /// HTTP 路径（路由模板，如 `"/v1/chat/completions"`），用于 tracking `ai.request.endpoint`。
    pub endpoint: String,
    /// 入口协议标识，用于 tracking `ai.request.request_format`。
    pub format: IngressFormat,
    /// 客户端 token 上下文（`AiAuthLayer` 已注入）。
    pub token: AiTokenContext,
    /// 是否流式。决定走 `invoke_stream_raw` + `transcode_stream` 还是 `invoke_non_stream`。
    pub is_stream: bool,
    /// 客户端请求里的 model 字段（映射前）。
    pub logical_model: String,
    /// 客户端 IP（从 header 里提取）。tracking 用。
    pub client_ip: String,
    /// 客户端 UA。tracking 用。
    pub user_agent: String,
    /// 入站 headers 脱敏快照。落 `ai.request.request_headers` 用。
    pub client_headers: Value,
    /// 客户端 wire 请求（将被消费，传给 `I::to_canonical`）。
    pub client_req: I::ClientRequest,
    /// 客户端 wire 请求的 JSON 快照——落 `ai.request.request_body` / `ai.log.content` 用。
    pub client_req_snapshot: Option<Value>,
    /// Reqwest 客户端（上游 HTTP 请求发送器）。
    pub http: reqwest::Client,
    /// 频道仓库（选路 + 状态缓存）。
    pub store: ChannelStore,
    /// 追踪服务（落 `ai.request` / `ai.request_execution` / `ai.log`）。
    pub tracking: TrackingService,
    /// 冷却服务（失败后写 `rate_limited_until` / `overload_until` / `disabled_api_keys`）。
    pub cooldown: CooldownService,
    /// 计费引擎（reserve/settle/refund）。`unlimited_quota` token 自动跳过。
    pub billing: BillingService,
    /// 价格解析器（`ai.channel_model_price` + `ai.group_ratio`）。
    pub price_resolver: PriceResolver,
}

/// 客户端响应的抽象 —— 交给 handler 包装成各自协议的 `Response`。
pub enum EngineOutcome<I: IngressConverter> {
    NonStream(I::ClientResponse),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>),
}

impl<I> PipelineCall<I>
where
    I: IngressConverter + Send + 'static,
    I::ClientRequest: Send,
    I::ClientResponse: serde::Serialize + Send,
    I::ClientStreamEvent: serde::Serialize + Send,
{
    /// 跑完整流程：选路 → 翻译 → 发上游 → 响应 + tracking。
    ///
    /// 非流式场景下开启跨候选 retry：按 `ChannelStore::candidates()` 返回的顺序遍历，
    /// 遇可重试错（`CrossChannel` / `SameChannel`）切下一个候选；遇 `Fatal` 立即终止。
    pub async fn execute(self) -> RelayResult<EngineOutcome<I>> {
        let Self {
            request_id,
            endpoint,
            format,
            token,
            is_stream,
            logical_model,
            client_ip,
            user_agent,
            client_headers,
            client_req,
            client_req_snapshot,
            http,
            store,
            tracking,
            cooldown,
            billing,
            price_resolver,
        } = self;

        let ctx = RelayContext::begin(
            request_id,
            token,
            endpoint,
            format,
            &logical_model,
            is_stream,
            ClientRequestMeta::new(client_ip, user_agent, client_headers),
        );
        let scope = EndpointScope::from(format);
        let service = chat::service_type_for(scope, is_stream);

        // ─── 1. 选路候选列表 ───────────────────────────────────────
        let candidates = match store.candidates(&logical_model, scope).await {
            Ok(list) => list,
            Err(e) => {
                tracking.emit_with_attempts(
                    ctx.clone(),
                    failure_outcome_from(&e),
                    Vec::new(),
                    client_req_snapshot,
                );
                return Err(e);
            }
        };
        if candidates.is_empty() {
            let err = RelayError::NoAvailableChannel {
                model: logical_model.clone(),
            };
            tracking.emit_with_attempts(
                ctx.clone(),
                failure_outcome_from(&err),
                Vec::new(),
                client_req_snapshot,
            );
            return Err(err);
        }

        // ─── 2. to_canonical（用第一个候选的 ingress_ctx；canonical 是中性表达）
        let first = &candidates[0];
        let first_target = match store
            .build_service_target(
                &http,
                &first.channel,
                &first.account,
                &first.selected_key,
                &logical_model,
                scope,
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracking.emit_with_attempts(
                    ctx.clone(),
                    failure_outcome_from(&e),
                    Vec::new(),
                    client_req_snapshot,
                );
                return Err(e);
            }
        };
        let first_ingress_ctx = IngressCtx::new(
            first_target.kind(),
            &logical_model,
            first_target.actual_model(),
        );
        let mut canonical_req = match I::to_canonical(client_req, &first_ingress_ctx) {
            Ok(r) => r,
            Err(e) => {
                let err = RelayError::from(e);
                tracking.emit_with_attempts(
                    ctx.clone(),
                    failure_outcome_from(&err),
                    Vec::new(),
                    client_req_snapshot,
                );
                return Err(err);
            }
        };
        canonical_req.stream = is_stream;
        let upstream_req_snapshot = serde_json::to_value(&canonical_req).ok();

        // ─── 3. billing reserve（unlimited_quota 跳过） ───────────────
        let billing_guard = match prepare_billing(
            &billing,
            &price_resolver,
            &ctx,
            first.channel.id,
            &logical_model,
            &canonical_req,
        )
        .await
        {
            Ok(g) => g,
            Err(e) => {
                tracking.emit_with_attempts(
                    ctx.clone(),
                    failure_outcome_from(&e),
                    Vec::new(),
                    client_req_snapshot,
                );
                return Err(e);
            }
        };

        // ─── 4. 分流：stream / non-stream ───────────────────────────
        if is_stream {
            // 流式路径保持**单 attempt**：用第一个候选，不 retry。
            let candidate = first.clone();
            let target = first_target;
            let kind = target.kind();
            execute_stream::<I>(
                ctx,
                candidate,
                kind,
                target,
                service,
                canonical_req,
                upstream_req_snapshot,
                client_req_snapshot,
                http,
                tracking,
                cooldown,
                billing_guard,
            )
            .await
        } else {
            execute_non_stream_with_retry::<I>(
                ctx,
                candidates,
                &logical_model,
                store,
                service,
                canonical_req,
                upstream_req_snapshot,
                client_req_snapshot,
                http,
                tracking,
                cooldown,
                billing_guard,
            )
            .await
        }
    }
}

// ---------------------------------------------------------------------------
// 非流式 + retry 循环
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn execute_non_stream_with_retry<I>(
    mut ctx: RelayContext,
    candidates: Vec<Candidate>,
    logical_model: &str,
    store: ChannelStore,
    service: summer_ai_core::ServiceType,
    canonical_req: summer_ai_core::ChatRequest,
    upstream_req_snapshot: Option<Value>,
    client_req_snapshot: Option<Value>,
    http: reqwest::Client,
    tracking: TrackingService,
    cooldown: CooldownService,
    mut billing_guard: Option<BillingGuard>,
) -> RelayResult<EngineOutcome<I>>
where
    I: IngressConverter + Send + 'static,
    I::ClientResponse: serde::Serialize + Send,
{
    let mut attempts: Vec<AttemptRecord> = Vec::new();
    let mut last_err: Option<RelayError> = None;

    for (idx, candidate) in candidates.into_iter().enumerate() {
        let attempt_no = (idx + 1) as i32;
        let attempt_start = Instant::now();
        let attempt_started_at = chrono::Utc::now().fixed_offset();

        let target = match store
            .build_service_target(
                &http,
                &candidate.channel,
                &candidate.account,
                &candidate.selected_key,
                logical_model,
                EndpointScope::from(ctx.format),
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                // target 构建错属 Fatal（例如 OAuth 未实装 / key 为空）
                attempts.push(mk_attempt_no_upstream(
                    attempt_no,
                    &candidate,
                    String::new(),
                    String::new(),
                    upstream_req_snapshot.clone(),
                    &e,
                    attempt_started_at,
                    attempt_start,
                ));
                last_err = Some(e);
                break;
            }
        };
        let kind = target.kind();

        let cost_profile = AdapterDispatcher::cost_profile(kind);
        ctx.attach_channel(
            &candidate.channel,
            &candidate.account,
            &candidate.selected_key,
            kind,
            cost_profile,
            target.actual_model().to_string(),
        );

        let ingress_ctx = IngressCtx::new(kind, &ctx.logical_model, target.actual_model());

        tracing::debug!(
            request_id = %ctx.request_id,
            attempt_no,
            channel_id = candidate.channel.id,
            account_id = candidate.account.id,
            adapter = kind.as_lower_str(),
            "pipeline attempt starting"
        );

        let mut sent_headers_sink: Option<Value> = None;
        let invoke_result = chat::invoke_non_stream(
            &http,
            kind,
            &target,
            service,
            &canonical_req,
            &mut sent_headers_sink,
        )
        .await;
        let sent_headers = sent_headers_sink
            .take()
            .unwrap_or(Value::Object(Default::default()));
        let attempt_duration = ms_since(attempt_start);
        let attempt_finished_at = chrono::Utc::now().fixed_offset();

        match invoke_result {
            Ok(invoked) => {
                let upstream_request_id = invoked.upstream_request_id.clone();
                if let Some(id) = upstream_request_id.clone() {
                    ctx.upstream_request_id = Some(id);
                }
                if let Some(h) = Some(sent_headers.clone()) {
                    ctx.set_sent_headers(h);
                }
                let canonical_resp = invoked.inner;
                let usage = canonical_resp.usage.clone();
                let resp_snapshot = serde_json::to_value(&canonical_resp).ok();

                attempts.push(AttemptRecord {
                    attempt_no,
                    channel_id: candidate.channel.id,
                    account_id: candidate.account.id,
                    request_format: kind.as_str().to_string(),
                    upstream_model: target.actual_model().to_string(),
                    upstream_request_id: upstream_request_id.clone(),
                    sent_headers,
                    request_body: upstream_req_snapshot.clone(),
                    response_body: resp_snapshot.clone(),
                    response_status_code: 200,
                    success: true,
                    error_message: String::new(),
                    duration_ms: attempt_duration,
                    first_token_ms: 0,
                    started_at: attempt_started_at,
                    finished_at: attempt_finished_at,
                });

                cooldown.record_success(candidate.account.id);

                let client_resp = match I::from_canonical(canonical_resp, &ingress_ctx) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = RelayError::from(e);
                        finalize_refund(
                            billing_guard.take(),
                            &ctx.request_id,
                            "egress_conversion_failed",
                        )
                        .await;
                        tracking.emit_with_attempts(
                            ctx,
                            failure_outcome_from(&err),
                            attempts,
                            client_req_snapshot,
                        );
                        return Err(err);
                    }
                };

                finalize_settle(billing_guard.take(), &usage, &ctx.request_id).await;

                let outcome = TrackingOutcome::Success {
                    upstream_status: 200,
                    usage,
                    response_snapshot: resp_snapshot,
                    upstream_request_id,
                };
                tracking.emit_with_attempts(ctx, outcome, attempts, client_req_snapshot);
                return Ok(EngineOutcome::NonStream(client_resp));
            }
            Err(e) => {
                let upstream_status = extract_upstream_status(&e);
                let resp_snapshot = extract_upstream_body(&e);
                attempts.push(AttemptRecord {
                    attempt_no,
                    channel_id: candidate.channel.id,
                    account_id: candidate.account.id,
                    request_format: kind.as_str().to_string(),
                    upstream_model: target.actual_model().to_string(),
                    upstream_request_id: None,
                    sent_headers,
                    request_body: upstream_req_snapshot.clone(),
                    response_body: resp_snapshot,
                    response_status_code: upstream_status.unwrap_or(0) as i32,
                    success: false,
                    error_message: e.to_string(),
                    duration_ms: attempt_duration,
                    first_token_ms: 0,
                    started_at: attempt_started_at,
                    finished_at: attempt_finished_at,
                });

                apply_cooldown(
                    &cooldown,
                    candidate.account.id,
                    &candidate.selected_key,
                    upstream_status,
                    &e,
                );
                cooldown.record_failure(candidate.account.id, candidate.channel.id);

                match e.retry_kind() {
                    RetryKind::Fatal => {
                        tracing::debug!(
                            request_id = %ctx.request_id,
                            attempt_no,
                            error = %e,
                            "fatal error, aborting retry"
                        );
                        last_err = Some(e);
                        break;
                    }
                    RetryKind::SameChannel | RetryKind::CrossChannel => {
                        tracing::warn!(
                            request_id = %ctx.request_id,
                            attempt_no,
                            error = %e,
                            "retryable error, trying next candidate"
                        );
                        last_err = Some(e);
                        continue;
                    }
                }
            }
        }
    }

    let err = last_err.unwrap_or_else(|| RelayError::NoAvailableChannel {
        model: logical_model.to_string(),
    });
    finalize_refund(
        billing_guard.take(),
        &ctx.request_id,
        "all_candidates_failed",
    )
    .await;
    tracking.emit_with_attempts(
        ctx,
        failure_outcome_from(&err),
        attempts,
        client_req_snapshot,
    );
    Err(err)
}

// ---------------------------------------------------------------------------
// 流式路径：单 attempt（流一旦开始无法优雅切 channel）
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn execute_stream<I>(
    mut ctx: RelayContext,
    candidate: Candidate,
    kind: summer_ai_core::AdapterKind,
    target: summer_ai_core::ServiceTarget,
    service: summer_ai_core::ServiceType,
    canonical_req: summer_ai_core::ChatRequest,
    upstream_req_snapshot: Option<Value>,
    client_req_snapshot: Option<Value>,
    http: reqwest::Client,
    tracking: TrackingService,
    cooldown: CooldownService,
    mut billing_guard: Option<BillingGuard>,
) -> RelayResult<EngineOutcome<I>>
where
    I: IngressConverter + Send + 'static,
    I::ClientStreamEvent: serde::Serialize + Send,
{
    let cost_profile = AdapterDispatcher::cost_profile(kind);
    ctx.attach_channel(
        &candidate.channel,
        &candidate.account,
        &candidate.selected_key,
        kind,
        cost_profile,
        target.actual_model().to_string(),
    );

    let ingress_ctx = IngressCtx::new(kind, &ctx.logical_model, target.actual_model());

    let mut sent_headers_sink: Option<Value> = None;
    let invoke_result = chat::invoke_stream_raw(
        &http,
        kind,
        &target,
        service,
        &canonical_req,
        &mut sent_headers_sink,
    )
    .await;
    if let Some(h) = sent_headers_sink.take() {
        ctx.set_sent_headers(h);
    }
    let invoked = match invoke_result {
        Ok(u) => u,
        Err(e) => {
            let upstream_status = extract_upstream_status(&e);
            apply_cooldown(
                &cooldown,
                candidate.account.id,
                &candidate.selected_key,
                upstream_status,
                &e,
            );
            cooldown.record_failure(candidate.account.id, candidate.channel.id);
            finalize_refund(
                billing_guard.take(),
                &ctx.request_id,
                "upstream_stream_open_failed",
            )
            .await;
            tracking.emit(
                ctx.clone(),
                failure_outcome_from(&e),
                client_req_snapshot,
                upstream_req_snapshot,
            );
            return Err(e);
        }
    };
    cooldown.record_success(candidate.account.id);
    if let Some(id) = invoked.upstream_request_id.clone() {
        ctx.upstream_request_id = Some(id);
    }
    let upstream = invoked.inner;

    let (tx, rx) = oneshot::channel::<StreamOutcome>();
    let tracking_spawn = tracking.clone();
    let ctx_spawn = ctx.clone();
    let client_snap_spawn = client_req_snapshot;
    let upstream_snap_spawn = upstream_req_snapshot;
    let upstream_id_hdr = invoked.upstream_request_id;
    let billing_spawn = billing_guard.take();
    let request_id_spawn = ctx_spawn.request_id.clone();

    tokio::spawn(async move {
        let outcome = match rx.await {
            Ok(so) if so.error.is_some() => {
                finalize_refund(billing_spawn, &request_id_spawn, "upstream_stream_error").await;
                TrackingOutcome::Failure {
                    client_status: 502,
                    upstream_status: Some(so.upstream_status),
                    message: so.error.unwrap_or_else(|| "upstream stream error".into()),
                    response_snapshot: None,
                }
            }
            Ok(so) => {
                let usage = so.usage.unwrap_or_default();
                finalize_settle(billing_spawn, &usage, &request_id_spawn).await;
                TrackingOutcome::Success {
                    upstream_status: so.upstream_status,
                    usage,
                    response_snapshot: None,
                    upstream_request_id: so.upstream_request_id.or(upstream_id_hdr),
                }
            }
            Err(_) => {
                finalize_refund(billing_spawn, &request_id_spawn, "stream_aborted").await;
                TrackingOutcome::Failure {
                    client_status: 499,
                    upstream_status: None,
                    message: "stream aborted before completion".into(),
                    response_snapshot: None,
                }
            }
        };
        tracking_spawn.emit(ctx_spawn, outcome, client_snap_spawn, upstream_snap_spawn);
    });

    let body_stream = stream_driver::transcode_stream::<I>(upstream, kind, target, ingress_ctx, tx);
    Ok(EngineOutcome::Stream(Box::pin(body_stream)))
}

// ---------------------------------------------------------------------------
// 辅助：错误摘要 → TrackingOutcome::Failure
// ---------------------------------------------------------------------------

fn failure_outcome_from(err: &RelayError) -> TrackingOutcome {
    TrackingOutcome::Failure {
        client_status: err.status_code().as_u16(),
        upstream_status: extract_upstream_status(err),
        message: err.to_string(),
        response_snapshot: extract_upstream_body(err),
    }
}

fn extract_upstream_status(err: &RelayError) -> Option<u16> {
    use summer_ai_core::AdapterError;
    match err {
        RelayError::UpstreamStatus { status, .. } => Some(*status),
        RelayError::Adapter(AdapterError::UpstreamStatus { status, .. }) => Some(*status),
        _ => None,
    }
}

/// 按上游状态码写冷却：429 → rate_limited_until；503/529 → overload_until；401/403 → 坏 key 拉黑。
///
/// fire-and-forget，不阻塞 retry。
fn apply_cooldown(
    cooldown: &CooldownService,
    account_id: i64,
    selected_key: &str,
    upstream_status: Option<u16>,
    err: &RelayError,
) {
    let Some(status) = upstream_status else {
        return;
    };
    match status {
        429 => cooldown.mark_rate_limited(account_id, 30, err.to_string()),
        503 | 529 => cooldown.mark_overloaded(account_id, 60, status, err.to_string()),
        401 | 403 if !selected_key.is_empty() => cooldown.disable_key(
            account_id,
            selected_key.to_string(),
            status,
            err.to_string(),
        ),
        _ => {}
    }
}

fn extract_upstream_body(err: &RelayError) -> Option<Value> {
    use summer_ai_core::AdapterError;

    let bytes: &[u8] = match err {
        RelayError::UpstreamStatus { body, .. } => body,
        RelayError::Adapter(AdapterError::UpstreamStatus { message, .. }) => message.as_bytes(),
        _ => return None,
    };
    if bytes.is_empty() {
        return None;
    }
    const MAX_BODY_BYTES: usize = 16 * 1024;
    let trimmed = if bytes.len() > MAX_BODY_BYTES {
        &bytes[..MAX_BODY_BYTES]
    } else {
        bytes
    };
    match serde_json::from_slice::<Value>(trimmed) {
        Ok(v) => Some(v),
        Err(_) => {
            let s = String::from_utf8_lossy(trimmed).to_string();
            Some(serde_json::json!({ "raw": s }))
        }
    }
}

fn ms_since(start: Instant) -> i32 {
    start.elapsed().as_millis().min(i32::MAX as u128) as i32
}

/// 构造一条"没到发上游就挂"的 `AttemptRecord`（比如 `build_service_target` 失败）。
#[allow(clippy::too_many_arguments)]
fn mk_attempt_no_upstream(
    attempt_no: i32,
    candidate: &Candidate,
    request_format: String,
    upstream_model: String,
    request_body: Option<Value>,
    err: &RelayError,
    started_at: sea_orm::prelude::DateTimeWithTimeZone,
    start: Instant,
) -> AttemptRecord {
    AttemptRecord {
        attempt_no,
        channel_id: candidate.channel.id,
        account_id: candidate.account.id,
        request_format,
        upstream_model,
        upstream_request_id: None,
        sent_headers: Value::Object(Default::default()),
        request_body,
        response_body: None,
        response_status_code: 0,
        success: false,
        error_message: err.to_string(),
        duration_ms: ms_since(start),
        first_token_ms: 0,
        started_at,
        finished_at: chrono::Utc::now().fixed_offset(),
    }
}

// ---------------------------------------------------------------------------
// billing：reserve / settle / refund 的 pipeline 端胶水
// ---------------------------------------------------------------------------

/// 请求进入 retry 循环前，按第一个候选的渠道价 + token 所属 group_ratio 算出
/// 预扣 quota，调用 `BillingService::reserve`。
///
/// `unlimited_quota == true` 时直接返 `Ok(None)`，后续 settle/refund 都跳过。
///
/// 任何 billing 路径的错误（余额不足 / user_quota 被禁 / DB 错）映射到 [`RelayError`]，
/// 与 pipeline 的其它错误统一走 tracking 失败路径。
async fn prepare_billing(
    billing: &BillingService,
    price_resolver: &PriceResolver,
    ctx: &RelayContext,
    channel_id: i64,
    logical_model: &str,
    canonical_req: &summer_ai_core::ChatRequest,
) -> RelayResult<Option<BillingGuard>> {
    if ctx.token.unlimited_quota {
        return Ok(None);
    }

    let (base_price, price_reference) = price_resolver
        .resolve(channel_id, logical_model)
        .await
        .map_err(map_price_error)?;

    let ratio = price_resolver
        .resolve_group_ratio(&ctx.token.group_code_override)
        .await
        .map_err(map_price_error)?;

    let price = base_price.apply_ratio(&ratio);
    let estimated = estimate_quota(canonical_req, &price);

    let reservation = billing
        .reserve(ctx.token.user_id, estimated, &price_reference)
        .await
        .map_err(map_billing_error)?;

    Ok(Some(BillingGuard {
        billing: billing.clone(),
        reservation,
        price,
    }))
}

async fn finalize_settle(
    guard: Option<BillingGuard>,
    usage: &summer_ai_core::Usage,
    request_id: &str,
) {
    let Some(guard) = guard else {
        return;
    };
    let price_ref = guard.reservation.price_reference.clone();
    let CostBreakdown { quota, .. } = compute_cost(usage, &guard.price, &price_ref);
    if let Err(e) = guard
        .billing
        .settle(guard.reservation, quota, request_id)
        .await
    {
        tracing::warn!(request_id, error = ?e, "billing settle failed");
    }
}

async fn finalize_refund(guard: Option<BillingGuard>, request_id: &str, reason: &str) {
    let Some(guard) = guard else {
        return;
    };
    if let Err(e) = guard
        .billing
        .refund(guard.reservation, request_id, reason)
        .await
    {
        tracing::warn!(request_id, reason, error = ?e, "billing refund failed");
    }
}

fn map_billing_error(e: BillingError) -> RelayError {
    match e {
        BillingError::InsufficientQuota { .. }
        | BillingError::UserQuotaNotFound(_)
        | BillingError::UserQuotaNotUsable { .. } => RelayError::QuotaExhausted,
        BillingError::InvalidEstimatedQuota(n) => {
            tracing::error!(estimated = n, "billing reserve got invalid estimate");
            RelayError::QuotaExhausted
        }
        BillingError::Database(db) => RelayError::Database(db),
    }
}

fn map_price_error(e: summer_ai_billing::PriceError) -> RelayError {
    use summer_ai_billing::PriceError as PE;
    match e {
        PE::Database(db) => RelayError::Database(db),
        other => {
            // 价格表缺失 / schema 错 / 币种 / 计费模式不支持 → 对客户端等价于"服务不可用"
            tracing::error!(error = %other, "price resolve failed");
            RelayError::NoAvailableChannel {
                model: String::new(),
            }
        }
    }
}
