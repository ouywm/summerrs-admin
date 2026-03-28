use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{FinishReason, StreamOptions, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<i32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_of: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionChoice {
    pub text: String,
    pub index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CompletionRequest {
    pub fn estimate_prompt_tokens(&self) -> i32 {
        estimate_prompt_tokens(&self.prompt).max(1)
    }
}

fn estimate_prompt_tokens(prompt: &serde_json::Value) -> i32 {
    match prompt {
        serde_json::Value::String(text) => ((text.len() as f64) / 4.0).ceil() as i32,
        serde_json::Value::Array(items) => items.iter().map(estimate_prompt_tokens).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_request_estimates_tokens_from_prompt_array() {
        let req: CompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-3.5-turbo-instruct",
            "prompt": ["hello world", "second line"]
        }))
        .unwrap();

        assert_eq!(req.estimate_prompt_tokens(), 6);
    }

    #[test]
    fn completion_chunk_deserializes_usage() {
        let chunk: CompletionChunk = serde_json::from_value(serde_json::json!({
            "id": "cmpl-123",
            "object": "text_completion",
            "created": 1700000000,
            "model": "gpt-3.5-turbo-instruct",
            "choices": [{"text": "hello", "index": 0, "finish_reason": null}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
        }))
        .unwrap();

        assert_eq!(chunk.choices[0].text, "hello");
        assert_eq!(chunk.usage.unwrap().total_tokens, 3);
    }
}
