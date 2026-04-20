//! `/v1/chat/completions` 业务逻辑。
//!
//! Handler 只做 HTTP 解包/封包；实际"发上游 + 解析响应"走这里。
//!
//! # 走路骨架
//!
//! - 非流式：`invoke_non_stream` → `AdapterDispatcher::build → reqwest send → parse_chat_response`
//! - 流式：`invoke_stream_raw` → 返上游 `reqwest::Response`，handler 自己 `bytes_stream()`
//!   **原样透传** SSE bytes。多入口协议时再做 canonical 重组。

use bytes::Bytes;
use serde_json::Value;
use summer_ai_core::{
    AdapterDispatcher, AdapterKind, ChatRequest, ChatResponse, ServiceTarget, ServiceType,
};

use crate::error::{RelayError, RelayResult};
use crate::extract::{extract_upstream_request_id, sanitize_reqwest_headers};

/// 成功路径的返回：上游响应 / 脱敏的请求 header 快照 / 响应里抽到的上游 request-id。
pub struct Invoked<T> {
    pub inner: T,
    pub upstream_request_id: Option<String>,
}

/// 非流式 chat：build → post → parse。
///
/// `sent_headers_sink` 在**发出上游请求之前**就会被填（成功和失败都填），供
/// tracking 落库 `ai.request_execution.request_headers`。失败时上游直接拒，
/// 这个字段仍然能说明"我们发过去的是什么 header"，对排查反测活 / 鉴权错尤其有用。
pub async fn invoke_non_stream(
    http: &reqwest::Client,
    kind: AdapterKind,
    target: &ServiceTarget,
    request: &ChatRequest,
    sent_headers_sink: &mut Option<Value>,
) -> RelayResult<Invoked<ChatResponse>> {
    let wire = AdapterDispatcher::build_chat_request(kind, target, ServiceType::Chat, request)?;
    *sent_headers_sink = Some(sanitize_reqwest_headers(&wire.headers));

    tracing::debug!(
        adapter = %kind.as_lower_str(),
        url = %wire.url,
        model_actual = %target.actual_model,
        "dispatch chat (non-stream)"
    );

    let response = http
        .post(&wire.url)
        .headers(wire.headers)
        .json(&wire.payload)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(upstream_error(response).await);
    }

    let upstream_request_id = extract_upstream_request_id(response.headers());
    let body = response.bytes().await?;
    let chat = AdapterDispatcher::parse_chat_response(kind, target, body)?;
    Ok(Invoked {
        inner: chat,
        upstream_request_id,
    })
}

/// 流式 chat：build → post，返上游 `reqwest::Response`。
///
/// `sent_headers_sink` 语义同 [`invoke_non_stream`]。
pub async fn invoke_stream_raw(
    http: &reqwest::Client,
    kind: AdapterKind,
    target: &ServiceTarget,
    request: &ChatRequest,
    sent_headers_sink: &mut Option<Value>,
) -> RelayResult<Invoked<reqwest::Response>> {
    let wire =
        AdapterDispatcher::build_chat_request(kind, target, ServiceType::ChatStream, request)?;
    *sent_headers_sink = Some(sanitize_reqwest_headers(&wire.headers));

    tracing::debug!(
        adapter = %kind.as_lower_str(),
        url = %wire.url,
        model_actual = %target.actual_model,
        "dispatch chat (stream)"
    );

    let response = http
        .post(&wire.url)
        .headers(wire.headers)
        .json(&wire.payload)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(upstream_error(response).await);
    }

    let upstream_request_id = extract_upstream_request_id(response.headers());
    Ok(Invoked {
        inner: response,
        upstream_request_id,
    })
}

/// 把 non-2xx 的 `reqwest::Response` 转成 `RelayError::UpstreamStatus`（保留原始 body）。
async fn upstream_error(response: reqwest::Response) -> RelayError {
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap_or_else(|_| Bytes::new());
    RelayError::UpstreamStatus { status, body }
}
