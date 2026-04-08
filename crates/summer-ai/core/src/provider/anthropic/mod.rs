use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream::BoxStream;
use reqwest::header::HeaderMap;

use self::convert::convert_response;
use self::protocol::{AnthropicResponse, anthropic_error_kind};
use self::request::{build_anthropic_body, build_anthropic_url};
use self::stream::AnthropicStreamMapper;
use crate::convert::message::responses_request_to_chat_request;
use crate::provider::{
    ChatProvider, Provider, ProviderErrorInfo, ProviderKind, ResponsesProvider,
    ResponsesRuntimeMode, status_to_provider_error_kind,
};
use crate::stream::mapped_chunk_stream;
use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

mod convert;
mod protocol;
mod request;
mod stream;

pub struct AnthropicAdapter;

impl Provider for AnthropicAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn parse_error(&self, status: u16, _headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo {
        let payload: serde_json::Value =
            serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
        let error_obj = payload.get("error").unwrap_or(&payload);
        let error_type = error_obj
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| default_anthropic_error_code(status));
        let message = error_obj
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());

        let kind = anthropic_error_kind(error_type)
            .unwrap_or_else(|| status_to_provider_error_kind(status));

        ProviderErrorInfo::new(kind, message, error_type)
    }
}

impl ChatProvider for AnthropicAdapter {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let body = build_anthropic_body(req, actual_model)?;

        Ok(client
            .post(build_anthropic_url(base_url))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body))
    }

    fn parse_chat_response(&self, body: Bytes, _model: &str) -> Result<ChatCompletionResponse> {
        let response: AnthropicResponse =
            serde_json::from_slice(&body).context("failed to deserialize anthropic response")?;
        Ok(convert_response(response))
    }

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        _model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        Ok(mapped_chunk_stream(response, AnthropicStreamMapper))
    }
}

impl ResponsesProvider for AnthropicAdapter {
    fn runtime_mode(&self) -> ResponsesRuntimeMode {
        ResponsesRuntimeMode::ChatBridge
    }

    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let req: crate::types::responses::ResponsesRequest = serde_json::from_value(req.clone())
            .context("failed to deserialize responses request")?;
        let chat_req = responses_request_to_chat_request(&req);
        self.build_chat_request(client, base_url, api_key, &chat_req, actual_model)
    }
}

fn default_anthropic_error_code(status: u16) -> &'static str {
    match status_to_provider_error_kind(status) {
        crate::provider::ProviderErrorKind::InvalidRequest => "invalid_request_error",
        crate::provider::ProviderErrorKind::Authentication => "authentication_error",
        crate::provider::ProviderErrorKind::RateLimit => "rate_limit_error",
        crate::provider::ProviderErrorKind::Server => "api_error",
        crate::provider::ProviderErrorKind::Api => "api_error",
    }
}

pub(super) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests;
