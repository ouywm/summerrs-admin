use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::Usage;

/// POST /v1/responses request body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesRequest {
    pub model: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Non-stream /v1/responses response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponseUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<ResponseInputTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ResponseOutputTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponseInputTokensDetails {
    #[serde(default)]
    pub cached_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponseOutputTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: i32,
}

impl ResponseUsage {
    pub fn to_usage(&self) -> Usage {
        Usage {
            prompt_tokens: self.input_tokens,
            completion_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            cached_tokens: self
                .input_tokens_details
                .as_ref()
                .map(|details| details.cached_tokens)
                .unwrap_or(0),
            reasoning_tokens: self
                .output_tokens_details
                .as_ref()
                .map(|details| details.reasoning_tokens)
                .unwrap_or(0),
        }
    }
}

pub fn estimate_input_tokens(input: &serde_json::Value) -> i32 {
    ((input.to_string().len() as f64) / 4.0).ceil() as i32
}

pub fn estimate_total_tokens_for_rate_limit(
    input: &serde_json::Value,
    max_output_tokens: Option<i64>,
) -> i64 {
    i64::from(estimate_input_tokens(input))
        + std::cmp::Ord::max(max_output_tokens.unwrap_or(2048), 1)
}

pub fn extract_response_usage(payload: &serde_json::Value) -> Option<Usage> {
    let response = payload.get("response").unwrap_or(payload);
    response
        .get("usage")
        .cloned()
        .and_then(|usage| serde_json::from_value::<ResponseUsage>(usage).ok())
        .map(|usage| usage.to_usage())
        .or_else(|| {
            serde_json::from_value::<ResponsesResponse>(response.clone())
                .ok()
                .and_then(|response| response.usage.map(|usage| usage.to_usage()))
        })
}

pub fn extract_response_model(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("response")
        .and_then(|response| response.get("model"))
        .or_else(|| payload.get("model"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

pub fn is_output_text_delta_event(payload: &serde_json::Value) -> bool {
    payload
        .get("type")
        .and_then(|value| value.as_str())
        .is_some_and(|event_type| event_type == "response.output_text.delta")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_round_trip() {
        let req: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": true,
            "max_output_tokens": 256,
            "metadata": {"project": "demo"},
            "custom_field": "custom"
        }))
        .unwrap();

        assert_eq!(req.model, "gpt-5.4");
        assert!(req.stream);
        assert_eq!(req.max_output_tokens, Some(256));
        assert_eq!(req.extra.get("custom_field").unwrap(), "custom");
    }

    #[test]
    fn extract_usage_from_response_object() {
        let payload = serde_json::json!({
            "id": "resp_123",
            "object": "response",
            "model": "gpt-5.4",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 34,
                "total_tokens": 46,
                "input_tokens_details": {"cached_tokens": 3},
                "output_tokens_details": {"reasoning_tokens": 5}
            }
        });

        let usage = extract_response_usage(&payload).unwrap();
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.completion_tokens, 34);
        assert_eq!(usage.total_tokens, 46);
        assert_eq!(usage.cached_tokens, 3);
        assert_eq!(usage.reasoning_tokens, 5);
    }

    #[test]
    fn extract_usage_from_completed_event() {
        let payload = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp_123",
                "object": "response",
                "model": "gpt-5.4",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 34,
                    "total_tokens": 46
                }
            }
        });

        let usage = extract_response_usage(&payload).unwrap();
        assert_eq!(usage.total_tokens, 46);
        assert_eq!(extract_response_model(&payload).as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn extract_usage_from_minimal_completed_event() {
        let payload = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp_123",
                "model": "gpt-5.4",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7,
                    "total_tokens": 19
                }
            }
        });

        let usage = extract_response_usage(&payload).unwrap();
        assert_eq!(usage.total_tokens, 19);
    }

    #[test]
    fn estimate_input_tokens_uses_json_size() {
        let input = serde_json::json!([
            {"role": "user", "content": "hello world"}
        ]);
        assert!(estimate_input_tokens(&input) > 0);
        assert!(estimate_total_tokens_for_rate_limit(&input, Some(512)) >= 512);
    }

    #[test]
    fn detects_output_text_delta_event() {
        assert!(is_output_text_delta_event(&serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "hello"
        })));
        assert!(!is_output_text_delta_event(&serde_json::json!({
            "type": "response.created"
        })));
    }
}
