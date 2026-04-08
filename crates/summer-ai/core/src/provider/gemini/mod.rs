use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream::BoxStream;
use reqwest::header::HeaderMap;

use self::convert::{convert_response, parse_embedding_response};
use self::embedding::{
    build_gemini_embedding_body, build_gemini_embedding_url, is_batch_embedding_input,
};
use self::protocol::GeminiResponse;
use self::request::{build_gemini_chat_body, build_gemini_url};
use self::stream::{GeminiStreamMapper, gemini_error_kind};
use crate::convert::message::responses_request_to_chat_request;
use crate::provider::{
    ChatProvider, EmbeddingProvider, Provider, ProviderErrorInfo, ProviderKind, ResponsesProvider,
    ResponsesRuntimeMode, status_to_provider_error_kind,
};
use crate::stream::mapped_chunk_stream;
use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};
use crate::types::embedding::{EmbeddingRequest, EmbeddingResponse};

mod convert;
mod embedding;
mod protocol;
mod request;
mod stream;

pub struct GeminiAdapter;

impl Provider for GeminiAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    fn parse_error(&self, status: u16, _headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo {
        let payload: serde_json::Value =
            serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
        let error_obj = payload.get("error").unwrap_or(&payload);
        let error_code = error_obj
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| default_gemini_error_code(status));
        let message = error_obj
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());

        let kind =
            gemini_error_kind(error_code).unwrap_or_else(|| status_to_provider_error_kind(status));

        ProviderErrorInfo::new(kind, message, error_code)
    }
}

impl ChatProvider for GeminiAdapter {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let body = build_gemini_chat_body(req)?;
        let url = if req.stream {
            format!("{}?alt=sse", build_gemini_url(base_url, actual_model, true))
        } else {
            build_gemini_url(base_url, actual_model, false)
        };

        Ok(client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body))
    }

    fn parse_chat_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse> {
        let response: GeminiResponse =
            serde_json::from_slice(&body).context("failed to deserialize gemini response")?;
        convert_response(response, model)
    }

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        Ok(mapped_chunk_stream(
            response,
            GeminiStreamMapper::new(model),
        ))
    }
}

impl ResponsesProvider for GeminiAdapter {
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

impl EmbeddingProvider for GeminiAdapter {
    fn build_embedding_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let req: EmbeddingRequest = serde_json::from_value(req.clone())
            .context("failed to deserialize embedding request")?;
        let body = build_gemini_embedding_body(&req, actual_model)?;
        let url = if is_batch_embedding_input(&req.input) {
            build_gemini_embedding_url(base_url, actual_model, true)
        } else {
            build_gemini_embedding_url(base_url, actual_model, false)
        };

        Ok(client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body))
    }

    fn parse_embedding_response(
        &self,
        body: Bytes,
        _model: &str,
        estimated_prompt_tokens: i32,
    ) -> Result<EmbeddingResponse> {
        parse_embedding_response(body, estimated_prompt_tokens)
    }
}

fn default_gemini_error_code(status: u16) -> &'static str {
    match status_to_provider_error_kind(status) {
        crate::provider::ProviderErrorKind::InvalidRequest => "INVALID_ARGUMENT",
        crate::provider::ProviderErrorKind::Authentication => "UNAUTHENTICATED",
        crate::provider::ProviderErrorKind::RateLimit => "RESOURCE_EXHAUSTED",
        crate::provider::ProviderErrorKind::Server => "INTERNAL",
        crate::provider::ProviderErrorKind::Api => "UNKNOWN",
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
