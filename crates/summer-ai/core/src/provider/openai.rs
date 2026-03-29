use anyhow::Result;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

use super::ProviderAdapter;

/// OpenAI 兼容适配器（零状态）
///
/// 直接透传请求体，仅替换 model 字段为映射后的实际模型名。
pub struct OpenAiAdapter;

impl ProviderAdapter for OpenAiAdapter {
    fn build_request(
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

    fn parse_response(&self, body: Bytes, _model: &str) -> Result<ChatCompletionResponse> {
        Ok(serde_json::from_slice(&body)?)
    }

    fn parse_stream(
        &self,
        response: reqwest::Response,
        _model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        let stream = async_stream::stream! {
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(anyhow::anyhow!("Stream read error: {e}"));
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // 按双换行分割 SSE 事件
                while let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    for line in event_text.lines() {
                        let line = line.trim();
                        if let Some(data) = line.strip_prefix("data:") {
                            let data = data.trim();
                            if data == "[DONE]" {
                                return;
                            }
                            if data.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<ChatCompletionChunk>(data) {
                                Ok(parsed) => yield Ok(parsed),
                                Err(e) => {
                                    tracing::warn!("Failed to parse SSE chunk: {e}, data: {data}");
                                }
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

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

    fn build_embeddings_request(
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
mod tests {
    use super::*;

    fn sample_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hello"}]
        }))
        .unwrap()
    }

    #[test]
    fn build_request_replaces_model() {
        let client = reqwest::Client::new();
        let adapter = OpenAiAdapter;
        let req = sample_request();

        let builder = adapter
            .build_request(
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

        // 验证 Authorization header
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
            .build_request(&client, "https://api.openai.com/", "sk-test", &req, "gpt-4")
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
            .build_request(
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
            .build_embeddings_request(
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
        let resp = adapter.parse_response(body, "gpt-4").unwrap();

        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn parse_response_invalid_json() {
        let adapter = OpenAiAdapter;
        let body = Bytes::from("not json");
        assert!(adapter.parse_response(body, "gpt-4").is_err());
    }

    #[tokio::test]
    async fn parse_stream_sse_chunks() {
        let adapter = OpenAiAdapter;

        // 模拟 SSE 响应
        let sse_body = concat!(
            "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        );

        // 创建 mock HTTP response
        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let stream = adapter.parse_stream(response, "gpt-4").unwrap();
        let chunks: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].choices[0].delta.content.as_deref(), Some("Hello"));
        assert_eq!(
            chunks[1].choices[0].delta.content.as_deref(),
            Some(" world")
        );
    }
}
