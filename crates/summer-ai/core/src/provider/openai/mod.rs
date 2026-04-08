use anyhow::Result;
use bytes::Bytes;
use futures::stream::BoxStream;

use crate::provider::error::parse_openai_compatible_error;
use crate::provider::{
    ChatProvider, EmbeddingProvider, Provider, ProviderErrorInfo, ProviderKind, ResponsesProvider,
};
use crate::stream::{SseEvent, StreamEventMapper, mapped_chunk_stream};
use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

/// OpenAI 兼容适配器（零状态）
///
/// 直接透传请求体，仅替换 model 字段为映射后的实际模型名。
pub struct OpenAiAdapter;

#[derive(Debug, Default, Clone, Copy)]
struct OpenAiStreamMapper;

#[derive(Debug, Default)]
struct OpenAiStreamState {
    stopped: bool,
}

impl StreamEventMapper for OpenAiStreamMapper {
    type State = OpenAiStreamState;

    fn map_event(
        &self,
        state: &mut Self::State,
        event: SseEvent,
    ) -> Vec<Result<ChatCompletionChunk>> {
        let data = event.data.trim();
        if data == "[DONE]" {
            state.stopped = true;
            return Vec::new();
        }
        if data.is_empty() {
            return Vec::new();
        }

        match serde_json::from_str::<ChatCompletionChunk>(data) {
            Ok(parsed) => vec![Ok(parsed)],
            Err(error) => {
                tracing::warn!("Failed to parse SSE chunk: {error}, data: {data}");
                Vec::new()
            }
        }
    }

    fn should_stop(&self, state: &Self::State) -> bool {
        state.stopped
    }
}

impl Provider for OpenAiAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }

    fn parse_error(
        &self,
        status: u16,
        _headers: &reqwest::header::HeaderMap,
        body: &[u8],
    ) -> ProviderErrorInfo {
        parse_openai_compatible_error(status, body)
    }
}

impl ChatProvider for OpenAiAdapter {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let mut body = serde_json::to_value(req)?;
        body["model"] = serde_json::Value::String(actual_model.to_string());

        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        Ok(client.post(url).bearer_auth(api_key).json(&body))
    }

    fn parse_chat_response(&self, body: Bytes, _model: &str) -> Result<ChatCompletionResponse> {
        Ok(serde_json::from_slice(&body)?)
    }

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        _model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        Ok(mapped_chunk_stream(response, OpenAiStreamMapper))
    }
}

impl ResponsesProvider for OpenAiAdapter {
    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let mut body = req.clone();
        body["model"] = serde_json::Value::String(actual_model.to_string());

        let url = format!("{}/v1/responses", base_url.trim_end_matches('/'));
        Ok(client.post(url).bearer_auth(api_key).json(&body))
    }
}

impl EmbeddingProvider for OpenAiAdapter {
    fn build_embedding_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let mut body = req.clone();
        body["model"] = serde_json::Value::String(actual_model.to_string());

        let url = format!("{}/v1/embeddings", base_url.trim_end_matches('/'));
        Ok(client.post(url).bearer_auth(api_key).json(&body))
    }
}

#[cfg(test)]
mod tests;
