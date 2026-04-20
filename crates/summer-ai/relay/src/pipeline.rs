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
//!   ChannelStore::pick ──(err)──► tracking.emit Failure → 抛 RelayError
//!       │
//!       ▼
//!   build_service_target + attach_channel
//!       │
//!       ▼
//!   IngressConverter::to_canonical
//!       │
//!       ├─── is_stream=false ───► invoke_non_stream
//!       │                             │
//!       │                             ├──(err)──► tracking.emit Failure
//!       │                             │
//!       │                             ▼
//!       │                         from_canonical → tracking.emit Success
//!       │                             │
//!       │                             ▼
//!       │                         EngineOutcome::NonStream
//!       │
//!       └─── is_stream=true ────► invoke_stream_raw
//!                                     │
//!                                     ├──(err)──► tracking.emit Failure
//!                                     │
//!                                     ▼
//!                                oneshot::channel<StreamOutcome>
//!                                tokio::spawn(等 rx → tracking.emit)
//!                                     │
//!                                     ▼
//!                                transcode_stream<I>(upstream, ..., tx)
//!                                     │
//!                                     ▼
//!                                EngineOutcome::Stream(BoxStream)
//! ```
//!
//! # 不做什么
//!
//! - **不做** billing（P6 加：在 `execute()` 的开头插 `reserve`，结束插 `settle/refund`）
//! - **不做** retry / failover（P9 加：在外面套一层 `Retryable` wrapper）
//! - **不做** egress format 最后包装（`sse_response` / `Json` 由 handler 自己做——
//!   因为 Gemini `streamGenerateContent` 有 SSE vs JSON-array 两种呈现模式）

use std::pin::Pin;

use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use tokio::sync::oneshot;

use summer_ai_core::AdapterDispatcher;

use crate::auth::AiTokenContext;
use crate::context::RelayContext;
use crate::convert::ingress::{IngressConverter, IngressCtx, IngressFormat};
use crate::error::{RelayError, RelayResult};
use crate::service::channel_store::{ChannelStore, build_service_target};
use crate::service::chat;
use crate::service::stream_driver::{self, StreamOutcome};
use crate::service::tracking::{TrackingOutcome, TrackingService};

/// 一次入口请求的所有上下文参数。
///
/// 字段接 by-value —— `reqwest::Client` / `ChannelStore` / `TrackingService` 都是
/// `Clone` 成本低的 handle（内部 `Arc`），每 handler 调用 clone 一次即可，避免
/// 给 `execute` 加生命周期参数。
pub struct PipelineCall<I: IngressConverter> {
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
    ///
    /// 序列化开销和原请求重复一次，v1 能忍。`None` 时只落空对象。
    pub client_req_snapshot: Option<Value>,
    /// Reqwest 客户端（上游 HTTP 请求发送器）。
    pub http: reqwest::Client,
    /// 频道仓库（选路 + 状态缓存）。
    pub store: ChannelStore,
    /// 追踪服务（落 `ai.request` / `ai.request_execution` / `ai.log`）。
    pub tracking: TrackingService,
}

/// 客户端响应的抽象 —— 交给 handler 包装成各自协议的 `Response`。
///
/// 非流式：直接是客户端 wire 响应类型（`ChatResponse` / `ClaudeResponse` / ...）；
/// 流式：一个 `BoxStream<Bytes>`，handler 自己决定 `sse_response` 还是 `collect_sse_to_json_array`。
pub enum EngineOutcome<I: IngressConverter> {
    /// 非流式响应 —— handler 用 `Json(r).into_response()` 直接返。
    NonStream(I::ClientResponse),
    /// 流式响应 —— handler 用 `Body::from_stream(s)` + `sse_response(body)` 封 SSE，
    /// 或者 `collect_sse_to_json_array(s)` 转 JSON-array（Gemini 默认）。
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
    /// 失败路径：**所有** `return Err(...)` 前都先 `tracking.emit(Failure, ...)`，保证
    /// DB 里能看到失败的 request / execution / log 记录。
    pub async fn execute(self) -> RelayResult<EngineOutcome<I>> {
        let Self {
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
        } = self;

        let mut ctx = RelayContext::begin(
            token,
            endpoint,
            format,
            &logical_model,
            is_stream,
            client_ip,
            user_agent,
            client_headers,
        );

        // ─── 1. 选路 ───────────────────────────────────────────────
        let picked = match store.pick(&logical_model).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                let err = RelayError::NoAvailableChannel {
                    model: logical_model.clone(),
                };
                emit_failure(&tracking, &ctx, &err, client_req_snapshot, None);
                return Err(err);
            }
            Err(e) => {
                emit_failure(&tracking, &ctx, &e, client_req_snapshot, None);
                return Err(e);
            }
        };
        let (channel, account, selected_key) = picked;

        let (kind, target) =
            match build_service_target(&channel, &account, &selected_key, &logical_model) {
                Ok(t) => t,
                Err(e) => {
                    emit_failure(&tracking, &ctx, &e, client_req_snapshot, None);
                    return Err(e);
                }
            };

        let cost_profile = AdapterDispatcher::cost_profile(kind);
        ctx.attach_channel(
            &channel,
            &account,
            &selected_key,
            kind,
            cost_profile,
            target.actual_model.clone(),
        );

        tracing::debug!(
            request_id = %ctx.request_id,
            endpoint = ctx.endpoint,
            format = format.as_str(),
            token_id = ctx.token.token_id,
            user_id = ctx.token.user_id,
            logical_model = %ctx.logical_model,
            actual_model = %target.actual_model,
            adapter = %kind.as_lower_str(),
            channel_id = channel.id,
            account_id = account.id,
            key_prefix = %&selected_key[..selected_key.len().min(6)],
            is_stream = ctx.is_stream,
            "relay pipeline prepared"
        );

        // ─── 2. ingress to_canonical ────────────────────────────────
        let ingress_ctx = IngressCtx::new(kind, &ctx.logical_model, &target.actual_model);
        let mut canonical_req = match I::to_canonical(client_req, &ingress_ctx) {
            Ok(r) => r,
            Err(e) => {
                let err = RelayError::from(e);
                emit_failure(&tracking, &ctx, &err, client_req_snapshot, None);
                return Err(err);
            }
        };
        canonical_req.stream = is_stream;

        let upstream_req_snapshot = serde_json::to_value(&canonical_req).ok();

        // ─── 3. 分流：stream / non-stream ───────────────────────────
        if is_stream {
            let mut sent_headers_sink: Option<Value> = None;
            let invoke_result = chat::invoke_stream_raw(
                &http,
                kind,
                &target,
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
                    emit_failure(
                        &tracking,
                        &ctx,
                        &e,
                        client_req_snapshot,
                        upstream_req_snapshot,
                    );
                    return Err(e);
                }
            };
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

            tokio::spawn(async move {
                // 等 transcode_stream 发来的最终态；客户端断连会 drop tx → rx 拿 RecvError。
                let outcome = match rx.await {
                    Ok(so) if so.error.is_some() => TrackingOutcome::Failure {
                        client_status: 502,
                        upstream_status: Some(so.upstream_status),
                        message: so.error.unwrap_or_else(|| "upstream stream error".into()),
                        response_snapshot: None,
                    },
                    Ok(so) => TrackingOutcome::Success {
                        upstream_status: so.upstream_status,
                        usage: so.usage.unwrap_or_default(),
                        response_snapshot: None,
                        // 流式：优先用 stream_driver 从流里嗅到的 id；否则回退到响应头上抽的 id
                        upstream_request_id: so.upstream_request_id.or(upstream_id_hdr),
                    },
                    Err(_) => TrackingOutcome::Failure {
                        client_status: 499,
                        upstream_status: None,
                        message: "stream aborted before completion".into(),
                        response_snapshot: None,
                    },
                };
                tracking_spawn.emit(ctx_spawn, outcome, client_snap_spawn, upstream_snap_spawn);
            });

            let body_stream =
                stream_driver::transcode_stream::<I>(upstream, kind, target, ingress_ctx, tx);
            Ok(EngineOutcome::Stream(Box::pin(body_stream)))
        } else {
            let mut sent_headers_sink: Option<Value> = None;
            let invoke_result = chat::invoke_non_stream(
                &http,
                kind,
                &target,
                &canonical_req,
                &mut sent_headers_sink,
            )
            .await;
            if let Some(h) = sent_headers_sink.take() {
                ctx.set_sent_headers(h);
            }
            let invoked = match invoke_result {
                Ok(r) => r,
                Err(e) => {
                    emit_failure(
                        &tracking,
                        &ctx,
                        &e,
                        client_req_snapshot,
                        upstream_req_snapshot,
                    );
                    return Err(e);
                }
            };
            let upstream_request_id = invoked.upstream_request_id;
            if let Some(id) = upstream_request_id.clone() {
                ctx.upstream_request_id = Some(id);
            }
            let canonical_resp = invoked.inner;

            let usage = canonical_resp.usage.clone();
            let resp_snapshot = serde_json::to_value(&canonical_resp).ok();

            let client_resp = match I::from_canonical(canonical_resp, &ingress_ctx) {
                Ok(r) => r,
                Err(e) => {
                    let err = RelayError::from(e);
                    emit_failure(
                        &tracking,
                        &ctx,
                        &err,
                        client_req_snapshot,
                        upstream_req_snapshot,
                    );
                    return Err(err);
                }
            };

            let outcome = TrackingOutcome::Success {
                upstream_status: 200,
                usage,
                response_snapshot: resp_snapshot,
                upstream_request_id,
            };
            tracking.emit(ctx, outcome, client_req_snapshot, upstream_req_snapshot);

            Ok(EngineOutcome::NonStream(client_resp))
        }
    }
}

/// 失败场景的统一 emit —— 从 `RelayError` 读 HTTP status、上游 body、信息摘要。
fn emit_failure(
    tracking: &TrackingService,
    ctx: &RelayContext,
    err: &RelayError,
    client_snap: Option<Value>,
    upstream_snap: Option<Value>,
) {
    let client_status = err.status_code().as_u16();
    let upstream_status = match err {
        RelayError::UpstreamStatus { status, .. } => Some(*status),
        RelayError::Adapter(summer_ai_core::AdapterError::UpstreamStatus { status, .. }) => {
            Some(*status)
        }
        _ => None,
    };
    let message = err.to_string();
    let response_snapshot = extract_upstream_body(err);
    let outcome = TrackingOutcome::Failure {
        client_status,
        upstream_status,
        message,
        response_snapshot,
    };
    tracking.emit(ctx.clone(), outcome, client_snap, upstream_snap);
}

/// 从上游错误里把 body 回收成 `Value` 供 tracking 落库。
///
/// - `RelayError::UpstreamStatus.body` 是原始 bytes；优先 JSON 解析，失败包 `{"raw": ...}`。
/// - `AdapterError::UpstreamStatus.message` 本身就是 body 的 UTF-8 字符串，同理处理。
/// - 其他 error（本地 DB / 鉴权 / 配置错）没有上游 body，返 None。
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
    // 限长，避免 16KB 以上的上游错误刷爆 JSONB（Postgres 内部压缩但仍耗空间）
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
