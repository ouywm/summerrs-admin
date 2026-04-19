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

use summer_ai_core::{AdapterDispatcher, AdapterKind, ChatStreamEvent, ServiceTarget};
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

/// 客户端 SSE 序列化格式。不同入口协议的 SSE 包装不同。
#[derive(Debug, Clone, Copy)]
pub enum ClientSseFormat {
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
            IngressFormat::Claude => Some(Self::Claude),
            IngressFormat::Gemini => Some(Self::Gemini),
            IngressFormat::OpenAIResponses => Some(Self::OpenAIResponses),
            _ => None,
        }
    }
}

/// Driver 的错误（不通过 `?` 直接返给客户端——handler 决定怎么处理）。
pub type DriverError = RelayError;

/// 把上游 HTTP 响应字节流重组成客户端 SSE 字节流。
///
/// `I` 是 Ingress converter（决定客户端 wire 格式）。`kind` 是上游 adapter
/// （决定怎么解析上游 SSE）。
///
/// 返回 Item 为 `Result<Bytes, std::io::Error>` 的流，适合
/// [`axum::body::Body::from_stream`]。
pub fn transcode_stream<I>(
    upstream: reqwest::Response,
    kind: AdapterKind,
    target: ServiceTarget,
    ctx: IngressCtx,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    I: IngressConverter + Send + 'static,
    I::ClientStreamEvent: Serialize + Send,
{
    // Ingress 决定序列化格式
    let client_format =
        ClientSseFormat::from_ingress_format(I::FORMAT).unwrap_or(ClientSseFormat::Claude);

    async_stream::stream! {
        let mut bytes = upstream.bytes_stream();
        let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
        let mut state = StreamConvertState::for_format(I::FORMAT);
        let mut upstream_done = false;

        while let Some(chunk) = bytes.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(?e, "upstream stream error");
                    yield Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("upstream: {e}"),
                    ));
                    return;
                }
            };
            buffer.extend_from_slice(&chunk);

            // 循环切 SSE event（用 \n\n 分隔）
            loop {
                let Some(end) = find_event_boundary(&buffer) else { break };
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

                // adapter 解析成 canonical
                let canonical: Option<ChatStreamEvent> =
                    match AdapterDispatcher::parse_chat_stream_event(kind, &target, &data) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(?e, "adapter parse_chat_stream_event error, skip");
                            continue;
                        }
                    };
                let Some(canonical_event) = canonical else { continue };

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

            if upstream_done {
                break;
            }
        }

        tracing::debug!("transcode_stream finished");
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
        ClientSseFormat::Claude | ClientSseFormat::OpenAIResponses => {
            // 从 JSON 里取 type 字段做 event name
            let event_name = extract_type_field(&json).unwrap_or("message");
            format!("event: {event_name}\ndata: {json}\n\n")
        }
        ClientSseFormat::Gemini => {
            format!("data: {json}\n\n")
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
                    parts: vec![GeminiPart::Text {
                        text: "hi".to_string(),
                    }],
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
}
