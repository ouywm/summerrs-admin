use super::*;
use crate::provider::{ChatProvider, EmbeddingProvider, ResponsesProvider};
use crate::stream::ChatStreamItem;
use futures::{StreamExt, stream};

fn sample_request() -> ChatCompletionRequest {
    serde_json::from_value(serde_json::json!({
        "model": "gpt-4",
        "messages": [{"role": "user", "content": "Hello"}]
    }))
    .unwrap()
}

fn chunk(item: &ChatStreamItem) -> &ChatCompletionChunk {
    item.chunk_ref().expect("expected chunk payload")
}

#[test]
fn build_request_replaces_model() {
    let client = reqwest::Client::new();
    let adapter = OpenAiAdapter;
    let req = sample_request();

    let builder = adapter
        .build_chat_request(
            &client,
            "https://api.openai.com",
            "sk-test",
            &req,
            "gpt-4-turbo",
        )
        .unwrap();

    let built = builder.build().unwrap();
    assert_eq!(
        built.url().as_str(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(built.method(), reqwest::Method::POST);

    let auth = built
        .headers()
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(auth, "Bearer sk-test");
}

#[test]
fn build_request_trims_trailing_slash() {
    let client = reqwest::Client::new();
    let adapter = OpenAiAdapter;
    let req = sample_request();

    let builder = adapter
        .build_chat_request(&client, "https://api.openai.com/", "sk-test", &req, "gpt-4")
        .unwrap();

    let built = builder.build().unwrap();
    assert_eq!(
        built.url().as_str(),
        "https://api.openai.com/v1/chat/completions"
    );
}

#[test]
fn build_request_body_contains_actual_model() {
    let client = reqwest::Client::new();
    let adapter = OpenAiAdapter;
    let req = sample_request();

    let builder = adapter
        .build_chat_request(
            &client,
            "https://api.example.com",
            "key",
            &req,
            "mapped-model",
        )
        .unwrap();

    let built = builder.build().unwrap();
    let body_bytes = built.body().unwrap().as_bytes().unwrap();
    let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
    assert_eq!(body["model"], "mapped-model");
    assert_eq!(body["messages"][0]["role"], "user");
}

#[test]
fn build_responses_request_uses_responses_endpoint() {
    let client = reqwest::Client::new();
    let adapter = OpenAiAdapter;
    let req = serde_json::json!({
        "model": "gpt-4.1",
        "input": "hello"
    });

    let builder = adapter
        .build_responses_request(
            &client,
            "https://api.openai.com/",
            "sk-test",
            &req,
            "gpt-4.1-mini",
        )
        .unwrap();

    let built = builder.build().unwrap();
    assert_eq!(built.url().as_str(), "https://api.openai.com/v1/responses");
    assert_eq!(built.method(), reqwest::Method::POST);

    let body_bytes = built.body().unwrap().as_bytes().unwrap();
    let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
    assert_eq!(body["model"], "gpt-4.1-mini");
    assert_eq!(body["input"], "hello");
}

#[test]
fn build_embeddings_request_uses_embeddings_endpoint() {
    let client = reqwest::Client::new();
    let adapter = OpenAiAdapter;
    let req = serde_json::json!({
        "model": "text-embedding-3-large",
        "input": "hello"
    });

    let builder = adapter
        .build_embedding_request(
            &client,
            "https://api.openai.com/",
            "sk-test",
            &req,
            "text-embedding-3-small",
        )
        .unwrap();

    let built = builder.build().unwrap();
    assert_eq!(built.url().as_str(), "https://api.openai.com/v1/embeddings");
    assert_eq!(built.method(), reqwest::Method::POST);

    let body_bytes = built.body().unwrap().as_bytes().unwrap();
    let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
    assert_eq!(body["model"], "text-embedding-3-small");
    assert_eq!(body["input"], "hello");
}

#[test]
fn parse_response_success() {
    let json = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1700000000,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "Hi!"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    });

    let adapter = OpenAiAdapter;
    let body = Bytes::from(serde_json::to_vec(&json).unwrap());
    let resp = adapter.parse_chat_response(body, "gpt-4").unwrap();

    assert_eq!(resp.id, "chatcmpl-123");
    assert_eq!(resp.choices.len(), 1);
    assert_eq!(resp.usage.total_tokens, 15);
}

#[test]
fn parse_response_invalid_json() {
    let adapter = OpenAiAdapter;
    let body = Bytes::from("not json");
    assert!(adapter.parse_chat_response(body, "gpt-4").is_err());
}

#[tokio::test]
async fn parse_stream_sse_chunks() {
    let adapter = OpenAiAdapter;
    let sse_body = concat!(
        "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let stream = adapter.parse_chat_stream(response, "gpt-4").unwrap();
    let chunks: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    assert_eq!(chunks.len(), 3);
    assert_eq!(
        chunk(&chunks[0]).choices[0].delta.content.as_deref(),
        Some("Hello")
    );
    assert_eq!(
        chunk(&chunks[1]).choices[0].delta.content.as_deref(),
        Some(" world")
    );
    assert!(chunks[2].is_terminal());
    assert!(chunks[2].chunk_ref().is_none());
}

#[tokio::test]
async fn parse_stream_preserves_utf8_when_sse_chunk_splits_multibyte_boundary() {
    let adapter = OpenAiAdapter;
    let event = concat!(
        "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,",
        "\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"你好\"},",
        "\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n"
    );
    let bytes = event.as_bytes();
    let split_at = bytes
        .windows("你".len())
        .position(|window| window == "你".as_bytes())
        .expect("utf8 boundary")
        + 1;
    let chunks = vec![
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&bytes[..split_at])),
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&bytes[split_at..])),
    ];
    let mock_response = http::Response::builder()
        .status(200)
        .body(reqwest::Body::wrap_stream(stream::iter(chunks)))
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "gpt-4")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(chunks.len(), 2);
    assert_eq!(
        chunk(&chunks[0]).choices[0].delta.content.as_deref(),
        Some("你好")
    );
    assert!(chunks[1].is_terminal());
    assert!(chunks[1].chunk_ref().is_none());
}
