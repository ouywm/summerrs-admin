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
use summer_ai_core::{
    AdapterDispatcher, AdapterKind, ChatRequest, ChatResponse, ServiceTarget, ServiceType,
};

use crate::error::{RelayError, RelayResult};

/// 非流式 chat：build → post → parse。
pub async fn invoke_non_stream(
    http: &reqwest::Client,
    kind: AdapterKind,
    target: &ServiceTarget,
    request: &ChatRequest,
) -> RelayResult<ChatResponse> {
    let wire = AdapterDispatcher::build_chat_request(kind, target, ServiceType::Chat, request)?;

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

    let body = response.bytes().await?;
    Ok(AdapterDispatcher::parse_chat_response(kind, target, body)?)
}

/// 流式 chat：build → post，返上游 `reqwest::Response`。
///
/// Handler 用 `response.bytes_stream()` 原样透传给客户端。
/// **当前阶段**：不做 canonical event 重组（多入口协议时加）。
pub async fn invoke_stream_raw(
    http: &reqwest::Client,
    kind: AdapterKind,
    target: &ServiceTarget,
    request: &ChatRequest,
) -> RelayResult<reqwest::Response> {
    let wire =
        AdapterDispatcher::build_chat_request(kind, target, ServiceType::ChatStream, request)?;

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

    Ok(response)
}

/// 把 non-2xx 的 `reqwest::Response` 转成 `RelayError::UpstreamStatus`（保留原始 body）。
async fn upstream_error(response: reqwest::Response) -> RelayError {
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap_or_else(|_| Bytes::new());
    RelayError::UpstreamStatus { status, body }
}
