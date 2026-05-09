//! 共享 SSE stream driver。
//!
//! 把上游的 `reqwest::Response` 字节流重组成客户端期望的 SSE 格式，中间过 canonical。
//!
//! # 管线
//!
//! ```text
//! upstream bytes ─┐
//!                 ├──► UTF-8 buffer ──► SSE event split (\n\n)
//!                 │                     │
//!                 │                     ├── data: [DONE] → 终止
//!                 │                     └── data: {json} → AdapterDispatcher::parse_chat_stream_event
//!                 │                                        │
//!                 │                                        ▼
//!                 │                                   canonical ChatStreamEvent
//!                 │                                        │
//!                 │                                        ▼
//!                 │                              I::from_canonical_stream_event
//!                 │                                        │
//!                 │                                        ▼
//!                 │                                Vec<ClientStreamEvent>
//!                 │                                        │
//!                 │                                        ▼
//!                 └──────────────────────────► 序列化成客户端 SSE bytes
//! ```
//!
//! # 客户端 SSE 格式
//!
//! - **Claude**：`event: {type}\ndata: {json}\n\n`（SSE 标准 named event）
//! - **Gemini**：`data: {json}\n\n`（带 `?alt=sse` 的 Gemini 响应格式）

use bytes::Bytes;
use futures::StreamExt;
use futures::stream::Stream;
use serde::Serialize;
use tokio::sync::oneshot;

use summer_ai_core::{
    AdapterDispatcher, AdapterKind, ChatStreamEvent, CompletionTokensDetails, PromptTokensDetails,
    ServiceTarget, StreamError, Usage,
};
use summer_web::axum::body::Body;
use summer_web::axum::http::{HeaderValue, StatusCode, header};
use summer_web::axum::response::{IntoResponse, Response};

use crate::convert::ingress::{IngressConverter, IngressCtx, IngressFormat, StreamConvertState};
use crate::error::RelayError;

// ---------------------------------------------------------------------------
// SSE 响应构造器（共用给 4 个流式 handler）
// ---------------------------------------------------------------------------

/// 统一 SSE 响应：`200 OK` + `text/event-stream` + `no-cache` + `keep-alive`。
///
/// 给 `/v1/chat/completions` / `/v1/messages` / `/v1beta/.../generateContent` /
/// `/v1/responses` 四个流式入口共用——header 三件套 + body。
pub fn sse_response(body: Body) -> Response {
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            (header::CONNECTION, HeaderValue::from_static("keep-alive")),
        ],
        body,
    )
        .into_response()
}

/// 把 [`transcode_stream`] 产出的 SSE 字节流收敛成 `Vec<Value>`——专给
/// Gemini `streamGenerateContent` 默认（不带 `?alt=sse`）的 JSON-array 响应模式用。
///
/// 把整个上游流消费完再返回，内存占用 = 所有 chunk JSON 合计体积。对
/// generateContent 通常十几到几十 KB，可接受。
pub async fn collect_sse_to_json_array<S>(stream: S) -> Result<Vec<serde_json::Value>, RelayError>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send,
{
    // async_stream! 生成的类型不 Unpin，这里 box-pin 一次即可 —— 只发生一次，
    // 跟整个 stream 收敛的 I/O 开销比可忽略。
    let mut stream = Box::pin(stream);
    let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
    while let Some(item) = stream.next().await {
        match item {
            Ok(b) => buf.extend_from_slice(&b),
            Err(e) => {
                tracing::warn!(%e, "upstream stream read error while collecting JSON array");
                break;
            }
        }
    }
    let text = std::str::from_utf8(&buf)
        .map_err(|e| RelayError::StreamProcessing(format!("non-utf8 bytes in SSE: {e}")))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("data:") else {
            continue;
        };
        let j = rest.trim();
        if j.is_empty() || j == "[DONE]" {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(j)
            .map_err(|e| RelayError::StreamProcessing(format!("invalid SSE JSON chunk: {e}")))?;
        out.push(value);
    }
    Ok(out)
}

/// 客户端 SSE 序列化格式。不同入口协议的 SSE 包装不同。
#[derive(Debug, Clone, Copy)]
pub enum ClientSseFormat {
    /// `data: {json}\n\n`；流尾追加 `data: [DONE]\n\n`。
    OpenAI,
    /// `event: {type}\ndata: {json}\n\n`
    Claude,
    /// `data: {json}\n\n`
    Gemini,
    /// `event: {type}\ndata: {json}\n\n`（和 Claude 一样但语义独立，便于未来差异化）
    OpenAIResponses,
}

impl ClientSseFormat {
    pub fn from_ingress_format(f: IngressFormat) -> Option<Self> {
        match f {
            IngressFormat::OpenAI => Some(Self::OpenAI),
            IngressFormat::Claude => Some(Self::Claude),
            IngressFormat::Gemini => Some(Self::Gemini),
            IngressFormat::OpenAIResponses => Some(Self::OpenAIResponses),
        }
    }
}

/// Driver 的错误（不通过 `?` 直接返给客户端——handler 决定怎么处理）。
pub type DriverError = RelayError;

// ---------------------------------------------------------------------------
// StreamOutcome —— 流结束后给 handler（通过 oneshot）的最终态
// ---------------------------------------------------------------------------

/// 流跑完的最终状态，给 tracking / billing 的后台任务用。
///
/// 只在 stream 真正结束（正常 `[DONE]` 或 `End` 事件 或 传输错误）时填完发给
/// `outcome_tx`。客户端中途断连时 `oneshot::Sender` 会随 stream future 一起 drop，
/// handler 的 receiver 端会拿到 `RecvError`，按"aborted"处理即可。
#[derive(Debug, Clone, Default)]
pub struct StreamOutcome {
    /// 上游 HTTP 响应的 status code（200 / 非 200）。
    pub upstream_status: u16,
    /// 上游返的 `x-request-id` / `openai-request-id` 等 header（选一个有的）。
    pub upstream_request_id: Option<String>,
    /// 累积到 `ChatStreamEvent::End` 时抓下来的 usage（上游未发 usage 为 None）。
    pub usage: Option<Usage>,
    /// 传输层错误摘要（`reqwest::bytes_stream` 出错时填）。
    pub error: Option<String>,
}

/// 把上游 HTTP 响应字节流重组成客户端 SSE 字节流。
///
/// `I` 是 Ingress converter（决定客户端 wire 格式）。`kind` 是上游 adapter
/// （决定怎么解析上游 SSE）。
///
/// 返回 Item 为 `Result<Bytes, std::io::Error>` 的流，适合
/// [`axum::body::Body::from_stream`]。
///
/// `outcome_tx` —— 流跑完时单次 send 一个 [`StreamOutcome`]：handler 通常把
/// 对应 `Receiver` 交给一个后台 `tokio::spawn` 等 usage，再落 tracking / 走
/// billing settle。
pub fn transcode_stream<I>(
    upstream: reqwest::Response,
    kind: AdapterKind,
    target: ServiceTarget,
    ctx: IngressCtx,
    outcome_tx: oneshot::Sender<StreamOutcome>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    I: IngressConverter + Send + 'static,
    I::ClientStreamEvent: Serialize + Send,
{
    // Ingress 决定序列化格式
    let client_format =
        ClientSseFormat::from_ingress_format(I::FORMAT).unwrap_or(ClientSseFormat::Claude);

    // 流开始前先抓 status / upstream-request-id（后面 upstream 的 ownership 会被
    // `bytes_stream()` 吃掉，就拿不到了）
    let upstream_status = upstream.status().as_u16();
    let upstream_request_id = extract_upstream_request_id(upstream.headers());

    async_stream::stream! {
        let mut bytes = upstream.bytes_stream();
        let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
        let mut state = StreamConvertState::for_format(I::FORMAT);
        let mut upstream_done = false;
        let mut final_usage: Option<Usage> = None;
        // `tokio::sync::oneshot::Sender` 需要 move-send；这里放进 Option 让循环里
        // 任意一条终止路径都能 take() 后 send，避免 "move in loop"。
        let mut outcome_slot: Option<oneshot::Sender<StreamOutcome>> = Some(outcome_tx);

        while let Some(chunk) = bytes.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(?e, "upstream stream error");
                    if let Some(tx) = outcome_slot.take() {
                        let _ = tx.send(StreamOutcome {
                            upstream_status,
                            upstream_request_id: upstream_request_id.clone(),
                            usage: final_usage.clone(),
                            error: Some(format!("upstream: {e}")),
                        });
                    }
                    yield Err(std::io::Error::other(format!("upstream: {e}")));
                    return;
                }
            };
            buffer.extend_from_slice(&chunk);

            // 循环切 SSE event（用 \n\n 分隔）
            while let Some(end) = find_event_boundary(&buffer) {
                let event_bytes: Vec<u8> = buffer.drain(..end).collect();

                // 尝试 UTF-8 解码（允许失败时跳过单个 event）
                let event_str = match std::str::from_utf8(&event_bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        tracing::warn!("non-utf8 SSE event bytes, skipping");
                        continue;
                    }
                };

                // 提取 data 行的内容（合并多行 data:，按 SSE 规范）
                let Some(data) = extract_data_lines(event_str) else {
                    continue;
                };

                // 终止标记
                if data.trim() == "[DONE]" {
                    upstream_done = true;
                    break;
                }

                // adapter 解析成 canonical（一个上游 chunk 可能对应多个事件——
                // Mistral 等会把 content + finish_reason 同块发出，OpenAI 并行
                // 工具调用也可能在一个 chunk 里 emit 多个 ToolCallDelta）
                let canonical_events: Vec<ChatStreamEvent> =
                    match AdapterDispatcher::parse_chat_stream_event(kind, &target, &data) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(?e, "adapter parse_chat_stream_event error, skip");
                            continue;
                        }
                    };
                if canonical_events.is_empty() {
                    continue;
                }

                for canonical_event in canonical_events {
                    // 抓 usage ——
                    // - `UsageDelta`：上游中期派发（Claude message_start 的 input+cache
                    //   就走这里，prompt 侧齐全、output=0）；按字段 merge，不能覆盖。
                    // - `End.usage`：上游尾块（Claude message_delta 的 output；OpenAI
                    //   末尾 usage-only chunk 的完整 usage）。也走 merge，让先到的
                    //   prompt 数据不被 output-only 的 usage 覆盖成 0。
                    match &canonical_event {
                        ChatStreamEvent::UsageDelta(u) => {
                            final_usage = Some(merge_stream_usage(final_usage.take(), u));
                        }
                        ChatStreamEvent::End(end_evt) => {
                            if let Some(u) = &end_evt.usage {
                                final_usage = Some(merge_stream_usage(final_usage.take(), u));
                            }
                        }
                        _ => {}
                    }

                    // Error 事件：立刻终止流，置 Failure outcome。不再 yield 后续
                    // 事件，避免客户端收到正常的 completed 信号。
                    if let ChatStreamEvent::Error(ref err) = canonical_event {
                        tracing::warn!(
                            error.kind = ?err.kind,
                            error.message = %err.message,
                            "upstream stream error event"
                        );
                        if let Some(tx) = outcome_slot.take() {
                            let _ = tx.send(build_stream_error_outcome(
                                err,
                                upstream_status,
                                upstream_request_id.clone(),
                                final_usage.clone(),
                            ));
                        }
                        upstream_done = true;
                        break;
                    }

                    // ingress 转成客户端 wire events
                    let client_events = match I::from_canonical_stream_event(
                        canonical_event,
                        &mut state,
                        &ctx,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(?e, "ingress from_canonical_stream_event error, skip");
                            continue;
                        }
                    };

                    // 序列化 + yield
                    for evt in client_events {
                        match serialize_client_event(&evt, client_format) {
                            Ok(b) => yield Ok(b),
                            Err(e) => {
                                tracing::warn!(?e, "serialize client event error");
                                continue;
                            }
                        }
                    }
                }
            }

            if upstream_done {
                break;
            }
        }

        // OpenAI 协议约定流尾要显式发一个 `data: [DONE]\n\n` 告诉 SDK 停止读。
        // 其它协议（Claude message_stop / Gemini candidate.finish_reason /
        // Responses response.completed）的终止信号已由各自 Egress 在上面 yield 出去。
        //
        // `outcome_slot.is_none()` 表示已经提前送过 outcome（错误路径会 .take() + send
        // Failure），这时候不能再发 [DONE]——否则 OpenAI SDK 把 [DONE] 当正常完成，
        // 客户端看不到错误。
        if outcome_slot.is_some() && matches!(client_format, ClientSseFormat::OpenAI) {
            yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));
        }

        // 正常结束：送 outcome（错误路径已经 .take() 过，这里 Option 为 None 不执行）
        if let Some(tx) = outcome_slot.take() {
            let _ = tx.send(StreamOutcome {
                upstream_status,
                upstream_request_id,
                usage: final_usage,
                error: None,
            });
        }

        tracing::debug!("transcode_stream finished");
    }
}

/// 同协议字节透传：客户端直接接收上游原始 SSE bytes，但后台仍解析 canonical 事件，
/// 用于 tracking / billing 的 usage 与错误归因。
pub fn passthrough_stream(
    upstream: reqwest::Response,
    kind: AdapterKind,
    target: ServiceTarget,
    outcome_tx: oneshot::Sender<StreamOutcome>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    let upstream_status = upstream.status().as_u16();
    let upstream_request_id = extract_upstream_request_id(upstream.headers());

    async_stream::stream! {
        let mut bytes = upstream.bytes_stream();
        let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
        let mut final_usage: Option<Usage> = None;
        let mut outcome_slot: Option<oneshot::Sender<StreamOutcome>> = Some(outcome_tx);
        let mut stream_error: Option<String> = None;

        while let Some(chunk) = bytes.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let message = format!("upstream: {e}");
                    if let Some(tx) = outcome_slot.take() {
                        let _ = tx.send(StreamOutcome {
                            upstream_status,
                            upstream_request_id: upstream_request_id.clone(),
                            usage: final_usage.clone(),
                            error: Some(message.clone()),
                        });
                    }
                    yield Err(std::io::Error::other(message));
                    return;
                }
            };

            buffer.extend_from_slice(&chunk);
            yield Ok(chunk.clone());

            while let Some(end) = find_event_boundary(&buffer) {
                let event_bytes: Vec<u8> = buffer.drain(..end).collect();
                let event_str = match std::str::from_utf8(&event_bytes) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let Some(data) = extract_data_lines(event_str) else {
                    continue;
                };
                if data.trim() == "[DONE]" {
                    break;
                }

                let canonical_events = match AdapterDispatcher::parse_chat_stream_event(
                    kind,
                    &target,
                    &data,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(?e, "adapter parse_chat_stream_event error during passthrough");
                        continue;
                    }
                };

                for canonical_event in canonical_events {
                    match &canonical_event {
                        ChatStreamEvent::UsageDelta(u) => {
                            final_usage = Some(merge_stream_usage(final_usage.take(), u));
                        }
                        ChatStreamEvent::End(end_evt) => {
                            if let Some(u) = &end_evt.usage {
                                final_usage = Some(merge_stream_usage(final_usage.take(), u));
                            }
                        }
                        ChatStreamEvent::Error(err) => {
                            stream_error = Some(match &err.kind {
                                Some(k) => format!("upstream stream error ({k}): {}", err.message),
                                None => format!("upstream stream error: {}", err.message),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(tx) = outcome_slot.take() {
            let _ = tx.send(StreamOutcome {
                upstream_status,
                upstream_request_id,
                usage: final_usage,
                error: stream_error,
            });
        }
    }
}

/// 从上游 response header 取"上游请求 id"。不同家命名不一样，按常见顺序尝试。
fn extract_upstream_request_id(headers: &reqwest::header::HeaderMap) -> Option<String> {
    for name in [
        "x-request-id",
        "openai-request-id",
        "anthropic-request-id",
        "x-amzn-requestid",
    ] {
        if let Some(v) = headers.get(name)
            && let Ok(s) = v.to_str()
        {
            return Some(s.to_string());
        }
    }
    None
}

/// 按字段 merge 两个 stream 过程中的 `Usage` 快照。
///
/// 为什么要 merge：Claude stream 上游的 prompt 侧 usage 在 `message_start` 发（对应
/// 我们的 `UsageDelta`，带 input_tokens + cache），completion 侧在 `message_delta`
/// 发（对应 `End.usage`，只带 output_tokens）。若直接覆盖，后到的 `End.usage`
/// 会把 prompt_tokens 拉回 0，billing 就看不到 prompt 花费。
///
/// Merge 规则：
/// - `prompt_tokens` / `completion_tokens`：取非零值（后到覆盖，但 0 不会抹掉非零）。
/// - `total_tokens`：merge 完后重算为 prompt + completion（上游给的 total 可能
///   在中期只算了部分，不足信）。
/// - `prompt_tokens_details` / `completion_tokens_details`：取非 `None`，内部
///   按字段同样"非零/非 None 覆盖"合并。
fn merge_stream_usage(prev: Option<Usage>, new: &Usage) -> Usage {
    let prev = prev.unwrap_or_default();
    let prompt_tokens = if new.prompt_tokens > 0 {
        new.prompt_tokens
    } else {
        prev.prompt_tokens
    };
    let completion_tokens = if new.completion_tokens > 0 {
        new.completion_tokens
    } else {
        prev.completion_tokens
    };
    let prompt_tokens_details = merge_prompt_tokens_details(
        prev.prompt_tokens_details,
        new.prompt_tokens_details.clone(),
    );
    let completion_tokens_details = merge_completion_tokens_details(
        prev.completion_tokens_details,
        new.completion_tokens_details.clone(),
    );
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        prompt_tokens_details,
        completion_tokens_details,
    }
}

fn merge_prompt_tokens_details(
    prev: Option<PromptTokensDetails>,
    new: Option<PromptTokensDetails>,
) -> Option<PromptTokensDetails> {
    match (prev, new) {
        (None, n) => n,
        (p, None) => p,
        (Some(p), Some(n)) => Some(PromptTokensDetails {
            cached_tokens: n.cached_tokens.or(p.cached_tokens),
            cache_creation_tokens: n.cache_creation_tokens.or(p.cache_creation_tokens),
            audio_tokens: n.audio_tokens.or(p.audio_tokens),
        }),
    }
}

fn merge_completion_tokens_details(
    prev: Option<CompletionTokensDetails>,
    new: Option<CompletionTokensDetails>,
) -> Option<CompletionTokensDetails> {
    match (prev, new) {
        (None, n) => n,
        (p, None) => p,
        (Some(p), Some(n)) => Some(CompletionTokensDetails {
            reasoning_tokens: n.reasoning_tokens.or(p.reasoning_tokens),
            audio_tokens: n.audio_tokens.or(p.audio_tokens),
            accepted_prediction_tokens: n
                .accepted_prediction_tokens
                .or(p.accepted_prediction_tokens),
            rejected_prediction_tokens: n
                .rejected_prediction_tokens
                .or(p.rejected_prediction_tokens),
        }),
    }
}

/// 根据上游 SSE 里带出来的 `StreamError`，组装给 handler 的 `Failure` outcome。
///
/// 错误信息前缀 `upstream stream error` 让 tracking / billing 层一眼分辨
/// 这是上游中途抛错（非传输断开），`kind` 若有则附带（Claude 的
/// `overloaded_error` / OpenAI 的 `insufficient_quota` 等）。
fn build_stream_error_outcome(
    err: &StreamError,
    upstream_status: u16,
    upstream_request_id: Option<String>,
    usage: Option<Usage>,
) -> StreamOutcome {
    let error_msg = match &err.kind {
        Some(k) => format!("upstream stream error ({k}): {}", err.message),
        None => format!("upstream stream error: {}", err.message),
    };
    StreamOutcome {
        upstream_status,
        upstream_request_id,
        usage,
        error: Some(error_msg),
    }
}

/// 找到一个完整 SSE event 的结束位置（`\n\n` 之后的索引）。
///
/// 返回 `Some(end_inclusive)`——调用方 `drain(..end)` 即可消费一个完整 event
/// 且包含尾部的 `\n\n`。
fn find_event_boundary(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n").map(|pos| pos + 2)
}

/// 从一个完整 SSE event（多行）里提取所有 `data:` 行并拼接成一个字符串。
fn extract_data_lines(event_str: &str) -> Option<String> {
    let mut out = String::new();
    for line in event_str.lines() {
        let trimmed = line.trim_end_matches('\r');
        // 跳过注释
        if trimmed.starts_with(':') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("data:") {
            // SSE 规范允许 "data: xxx" 或 "data:xxx"
            let payload = rest.strip_prefix(' ').unwrap_or(rest);
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(payload);
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 把单个客户端 wire event 序列化成客户端 SSE bytes。
fn serialize_client_event<T: Serialize>(
    event: &T,
    format: ClientSseFormat,
) -> Result<Bytes, serde_json::Error> {
    let json = serde_json::to_string(event)?;
    let body = match format {
        ClientSseFormat::OpenAI | ClientSseFormat::Gemini => {
            format!("data: {json}\n\n")
        }
        ClientSseFormat::Claude | ClientSseFormat::OpenAIResponses => {
            // 从 JSON 里取 type 字段做 event name
            let event_name = extract_type_field(&json).unwrap_or("message");
            format!("event: {event_name}\ndata: {json}\n\n")
        }
    };
    Ok(Bytes::from(body))
}

/// 取 JSON 对象顶层的 `"type": "..."` 字段（不做完整解析，用于 Claude SSE
/// `event:` 行的值）。
fn extract_type_field(json: &str) -> Option<&str> {
    // 简单扫描："type":"xxx" 或 "type": "xxx"
    let bytes = json.as_bytes();
    let key = b"\"type\"";
    let idx = json.find("\"type\"")?;
    let mut i = idx + key.len();
    // 跳过空白和冒号
    while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b':') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;
    let start = i;
    while i < bytes.len() && bytes[i] != b'"' {
        i += 1;
    }
    Some(&json[start..i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_type_field_basic() {
        assert_eq!(
            extract_type_field(r#"{"type":"message_start","index":0}"#),
            Some("message_start")
        );
        assert_eq!(
            extract_type_field(r#"{"type": "content_block_delta"}"#),
            Some("content_block_delta")
        );
        assert_eq!(extract_type_field(r#"{"other":"x"}"#), None);
    }

    #[test]
    fn find_event_boundary_crlf_not_counted() {
        let buf = b"data: foo\n\ndata: bar";
        let end = find_event_boundary(buf).unwrap();
        assert_eq!(&buf[..end], b"data: foo\n\n");
    }

    #[test]
    fn find_event_boundary_none_when_incomplete() {
        let buf = b"data: foo\ndata: bar\n";
        assert!(find_event_boundary(buf).is_none());
    }

    #[test]
    fn extract_data_single_line() {
        assert_eq!(
            extract_data_lines("data: hello\n\n"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn extract_data_multi_data_lines_joined() {
        assert_eq!(
            extract_data_lines("data: first\ndata: second\n"),
            Some("first\nsecond".to_string())
        );
    }

    #[test]
    fn extract_data_ignores_event_name_and_comments() {
        assert_eq!(
            extract_data_lines(": keep-alive\nevent: foo\ndata: payload\n"),
            Some("payload".to_string())
        );
    }

    #[test]
    fn extract_data_empty_event_returns_none() {
        assert!(extract_data_lines(": comment only\nevent: foo\n").is_none());
    }

    #[test]
    fn serialize_claude_event_has_type_and_data_lines() {
        use summer_ai_core::types::ingress_wire::claude::{ClaudeStreamDelta, ClaudeStreamEvent};
        let evt = ClaudeStreamEvent::ContentBlockDelta {
            index: 0,
            delta: ClaudeStreamDelta::TextDelta {
                text: "hi".to_string(),
            },
        };
        let bytes = serialize_client_event(&evt, ClientSseFormat::Claude).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("event: content_block_delta\ndata: "));
        assert!(s.ends_with("\n\n"));
        assert!(s.contains(r#""text":"hi""#));
    }

    #[test]
    fn serialize_gemini_event_is_data_only() {
        use summer_ai_core::types::ingress_wire::gemini::{
            GeminiCandidate, GeminiChatResponse, GeminiContent, GeminiPart,
        };
        let chunk = GeminiChatResponse {
            candidates: vec![GeminiCandidate {
                index: 0,
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::plain_text("hi")],
                }),
                finish_reason: None,
                safety_ratings: Vec::new(),
                grounding_metadata: None,
                citation_metadata: None,
            }],
            prompt_feedback: None,
            usage_metadata: None,
            model_version: None,
        };
        let bytes = serialize_client_event(&chunk, ClientSseFormat::Gemini).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("data: "));
        assert!(s.ends_with("\n\n"));
        assert!(!s.contains("event:"));
    }

    #[test]
    fn build_stream_error_outcome_includes_kind_when_present() {
        // 上游带 kind 时，错误消息要附上括号形式的 kind，方便 tracking / billing
        // 区分不同类型的上游错误（overloaded_error / insufficient_quota 等）。
        let outcome = build_stream_error_outcome(
            &StreamError {
                message: "Overloaded".to_string(),
                kind: Some("overloaded_error".to_string()),
            },
            200,
            Some("req-123".to_string()),
            None,
        );
        assert_eq!(outcome.upstream_status, 200);
        assert_eq!(outcome.upstream_request_id.as_deref(), Some("req-123"));
        assert_eq!(
            outcome.error.as_deref(),
            Some("upstream stream error (overloaded_error): Overloaded")
        );
    }

    #[test]
    fn build_stream_error_outcome_omits_kind_parens_when_absent() {
        // 无 kind 时不要生成空括号 `()` —— 之前若直接 format 会出 `error (): msg`，
        // 日志检索很难看。
        let outcome = build_stream_error_outcome(
            &StreamError {
                message: "boom".to_string(),
                kind: None,
            },
            502,
            None,
            None,
        );
        assert_eq!(
            outcome.error.as_deref(),
            Some("upstream stream error: boom")
        );
        assert_eq!(outcome.upstream_status, 502);
    }

    #[test]
    fn build_stream_error_outcome_propagates_usage() {
        // 错误发生前已经累积的 usage 要带到 outcome，让 billing 层能按已产出的
        // tokens 退费/计费，而不是整单丢弃。
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            ..Default::default()
        };
        let outcome = build_stream_error_outcome(
            &StreamError {
                message: "x".to_string(),
                kind: None,
            },
            200,
            None,
            Some(usage),
        );
        let u = outcome.usage.expect("usage should be carried through");
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.completion_tokens, 5);
    }

    #[test]
    fn merge_stream_usage_preserves_prompt_tokens_when_followup_has_zero() {
        // Claude 的典型流式序列：
        //   1) message_start → UsageDelta{prompt=290, completion=0, cache=(200,80)}
        //   2) message_delta → End{usage{prompt=0, completion=42}}
        // 若按"后到覆盖"，第二步会把 prompt 冲成 0 —— billing 只看到 completion。
        // merge 必须保留第一步的 prompt / cache。
        let first = Usage {
            prompt_tokens: 290,
            completion_tokens: 0,
            total_tokens: 290,
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: Some(80),
                cache_creation_tokens: Some(200),
                audio_tokens: None,
            }),
            completion_tokens_details: None,
        };
        let second = Usage {
            prompt_tokens: 0,
            completion_tokens: 42,
            total_tokens: 42,
            ..Default::default()
        };

        let merged = merge_stream_usage(Some(first), &second);

        assert_eq!(merged.prompt_tokens, 290);
        assert_eq!(merged.completion_tokens, 42);
        assert_eq!(merged.total_tokens, 332);
        let details = merged.prompt_tokens_details.as_ref().unwrap();
        assert_eq!(details.cached_tokens, Some(80));
        assert_eq!(details.cache_creation_tokens, Some(200));
    }

    #[test]
    fn merge_stream_usage_newer_nonzero_overrides_older() {
        // OpenAI 的末尾 usage-only chunk 可能会把 prompt / completion 都重刷一遍，
        // 此时 new 的非零值要覆盖 prev（更权威）。
        let first = Usage {
            prompt_tokens: 5,
            completion_tokens: 3,
            total_tokens: 8,
            ..Default::default()
        };
        let second = Usage {
            prompt_tokens: 10,
            completion_tokens: 7,
            total_tokens: 17,
            ..Default::default()
        };
        let merged = merge_stream_usage(Some(first), &second);
        assert_eq!(merged.prompt_tokens, 10);
        assert_eq!(merged.completion_tokens, 7);
        assert_eq!(merged.total_tokens, 17);
    }

    #[test]
    fn merge_stream_usage_prev_none_uses_new() {
        let new = Usage {
            prompt_tokens: 5,
            completion_tokens: 2,
            total_tokens: 7,
            ..Default::default()
        };
        let merged = merge_stream_usage(None, &new);
        assert_eq!(merged.prompt_tokens, 5);
        assert_eq!(merged.completion_tokens, 2);
    }
}
