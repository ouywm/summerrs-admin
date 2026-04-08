use anyhow::Result;
use bytes::Bytes;
use futures::stream::BoxStream;
use reqwest::header::HeaderMap;

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};
use crate::types::embedding::EmbeddingResponse;

pub mod anthropic;
pub mod azure;
pub mod error;
pub mod gemini;
pub mod kind;
pub mod openai;
pub mod registry;

pub use anthropic::AnthropicAdapter;
pub use azure::AzureOpenAiAdapter;
pub use error::{
    ProviderErrorInfo, ProviderErrorKind, ProviderStreamError, status_to_provider_error_kind,
};
pub use gemini::GeminiAdapter;
pub use kind::ProviderKind;
pub use openai::OpenAiAdapter;
pub use registry::{ProviderMeta, ProviderRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesRuntimeMode {
    Native,
    ChatBridge,
}

pub trait Provider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind;

    fn parse_error(&self, status: u16, headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo;
}

pub trait ChatProvider: Provider {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    fn parse_chat_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse>;

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;
}

pub trait EmbeddingProvider: Provider {
    fn build_embedding_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    fn parse_embedding_response(
        &self,
        body: Bytes,
        _model: &str,
        _estimated_prompt_tokens: i32,
    ) -> Result<EmbeddingResponse> {
        serde_json::from_slice(&body).map_err(Into::into)
    }
}

pub trait ResponsesProvider: Provider {
    fn runtime_mode(&self) -> ResponsesRuntimeMode {
        ResponsesRuntimeMode::Native
    }

    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;
}

#[cfg(test)]
mod tests;
