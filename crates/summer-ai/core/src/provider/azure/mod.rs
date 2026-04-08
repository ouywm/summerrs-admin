use anyhow::{Result, anyhow};
use bytes::Bytes;
use futures::stream::BoxStream;

use crate::provider::error::parse_openai_compatible_error;
use crate::provider::{
    ChatProvider, EmbeddingProvider, OpenAiAdapter, Provider, ProviderErrorInfo, ProviderKind,
    ResponsesProvider,
};
use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

const DEFAULT_AZURE_API_VERSION: &str = "2024-10-21";

pub struct AzureOpenAiAdapter;

impl Provider for AzureOpenAiAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::AzureOpenAi
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

impl ChatProvider for AzureOpenAiAdapter {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let mut body = serde_json::to_value(req)?;

        if uses_openai_v1_base_url(base_url) {
            set_model(&mut body, actual_model);
            return Ok(
                azure_post(client, build_v1_url(base_url, "chat/completions"), api_key).json(&body),
            );
        }

        remove_model(&mut body);
        Ok(azure_post(
            client,
            build_legacy_deployment_url(base_url, actual_model, "chat/completions"),
            api_key,
        )
        .json(&body))
    }

    fn parse_chat_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse> {
        let delegate = OpenAiAdapter;
        <OpenAiAdapter as ChatProvider>::parse_chat_response(&delegate, body, model)
    }

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        let delegate = OpenAiAdapter;
        <OpenAiAdapter as ChatProvider>::parse_chat_stream(&delegate, response, model)
    }
}

impl ResponsesProvider for AzureOpenAiAdapter {
    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        if !uses_openai_v1_base_url(base_url) {
            return Err(anyhow!(
                "responses endpoint is not supported for Azure deployment endpoints"
            ));
        }

        let mut body = req.clone();
        set_model(&mut body, actual_model);

        Ok(azure_post(client, build_v1_url(base_url, "responses"), api_key).json(&body))
    }
}

impl EmbeddingProvider for AzureOpenAiAdapter {
    fn build_embedding_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let mut body = req.clone();

        if uses_openai_v1_base_url(base_url) {
            set_model(&mut body, actual_model);
            return Ok(
                azure_post(client, build_v1_url(base_url, "embeddings"), api_key).json(&body),
            );
        }

        remove_model(&mut body);
        Ok(azure_post(
            client,
            build_legacy_deployment_url(base_url, actual_model, "embeddings"),
            api_key,
        )
        .json(&body))
    }
}

#[cfg(test)]
mod tests;

fn uses_openai_v1_base_url(base_url: &str) -> bool {
    base_url.trim_end_matches('/').contains("/openai/v1")
}

fn build_v1_url(base_url: &str, operation: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    format!("{base_url}/{operation}")
}

fn build_legacy_deployment_url(base_url: &str, deployment: &str, operation: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    format!(
        "{base_url}/openai/deployments/{deployment}/{operation}?api-version={DEFAULT_AZURE_API_VERSION}"
    )
}

fn azure_post(client: &reqwest::Client, url: String, api_key: &str) -> reqwest::RequestBuilder {
    client.post(url).header("api-key", api_key)
}

fn set_model(body: &mut serde_json::Value, actual_model: &str) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "model".into(),
            serde_json::Value::String(actual_model.to_string()),
        );
    }
}

fn remove_model(body: &mut serde_json::Value) {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("model");
    }
}
