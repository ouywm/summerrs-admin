use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{Delta, FinishReason, Message, StreamOptions, Tool, Usage};

/// POST /v1/chat/completions 请求体
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    /// 透传未知字段
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 非流式响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

/// 非流式选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: i32,
    pub message: Message,
    pub finish_reason: Option<FinishReason>,
}

/// 流式 chunk（SSE data 行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// 流式选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: i32,
    pub delta: Delta,
    pub finish_reason: Option<FinishReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn request_minimal_deserialize() {
        let req = sample_request();
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.messages.len(), 2);
        assert!(!req.stream);
        assert!(req.temperature.is_none());
        assert!(req.tools.is_none());
    }

    #[test]
    fn request_stream_flag() {
        let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "stream_options": {"include_usage": true}
        }))
        .unwrap();
        assert!(req.stream);
        assert!(req.stream_options.unwrap().include_usage.unwrap());
    }

    #[test]
    fn request_extra_fields_preserved() {
        let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hi"}],
            "custom_field": "custom_value"
        }))
        .unwrap();
        assert_eq!(req.extra.get("custom_field").unwrap(), "custom_value");
    }

    #[test]
    fn request_round_trip() {
        let req = sample_request();
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatCompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, req.model);
        assert_eq!(parsed.messages.len(), req.messages.len());
    }

    #[test]
    fn response_deserialize() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let resp: ChatCompletionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.choices.len(), 1);
        assert!(matches!(
            resp.choices[0].finish_reason,
            Some(FinishReason::Stop)
        ));
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn chunk_deserialize() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        });
        let chunk: ChatCompletionChunk = serde_json::from_value(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn chunk_with_usage() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "gpt-4",
            "choices": [],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });
        let chunk: ChatCompletionChunk = serde_json::from_value(json).unwrap();
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.total_tokens, 30);
    }

    #[test]
    fn request_with_tools() {
        let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "weather?"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather info",
                    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
                }
            }],
            "tool_choice": "auto"
        }))
        .unwrap();
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        assert_eq!(req.tools.unwrap()[0].function.name, "get_weather");
    }
}
