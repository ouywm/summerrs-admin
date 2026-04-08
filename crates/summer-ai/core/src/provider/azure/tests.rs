use super::*;
use crate::provider::{ChatProvider, EmbeddingProvider, ResponsesProvider};
use crate::types::chat::ChatCompletionRequest;

#[test]
fn azure_legacy_chat_request_uses_api_key_header_and_deployment_path() {
    let client = reqwest::Client::new();
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "Hello"}]
    }))
    .unwrap();

    let built = AzureOpenAiAdapter
        .build_chat_request(
            &client,
            "https://example-resource.openai.azure.com",
            "azure-key",
            &req,
            "gpt-4o-deployment",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://example-resource.openai.azure.com/openai/deployments/gpt-4o-deployment/chat/completions?api-version=2024-10-21"
    );
    assert_eq!(
        built
            .headers()
            .get("api-key")
            .and_then(|value| value.to_str().ok()),
        Some("azure-key")
    );
    assert!(built.headers().get("authorization").is_none());
}

#[test]
fn azure_v1_responses_request_uses_openai_v1_base_url() {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "model": "gpt-4.1",
        "input": "hello"
    });

    let built = AzureOpenAiAdapter
        .build_responses_request(
            &client,
            "https://example-resource.openai.azure.com/openai/v1/",
            "azure-key",
            &payload,
            "gpt-4.1-deployment",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://example-resource.openai.azure.com/openai/v1/responses"
    );
    assert_eq!(
        built
            .headers()
            .get("api-key")
            .and_then(|value| value.to_str().ok()),
        Some("azure-key")
    );

    let body_bytes = built.body().unwrap().as_bytes().unwrap();
    let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
    assert_eq!(body["model"], "gpt-4.1-deployment");
}

#[test]
fn azure_legacy_embeddings_request_uses_api_key_header_and_deployment_path() {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "model": "text-embedding-3-large",
        "input": "hello"
    });

    let built = AzureOpenAiAdapter
        .build_embedding_request(
            &client,
            "https://example-resource.openai.azure.com",
            "azure-key",
            &payload,
            "text-embedding-3-large-deployment",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://example-resource.openai.azure.com/openai/deployments/text-embedding-3-large-deployment/embeddings?api-version=2024-10-21"
    );
    assert_eq!(
        built
            .headers()
            .get("api-key")
            .and_then(|value| value.to_str().ok()),
        Some("azure-key")
    );
}
