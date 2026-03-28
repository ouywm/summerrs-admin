use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModerationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input: serde_json::Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModerationResponse {
    pub id: String,
    pub model: String,
    pub results: Vec<ModerationResult>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModerationResult {
    pub flagged: bool,
    #[serde(default)]
    pub categories: serde_json::Value,
    #[serde(default)]
    pub category_scores: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_applied_input_types: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ModerationRequest {
    pub fn effective_model(&self) -> &str {
        self.model.as_deref().unwrap_or("omni-moderation-latest")
    }

    pub fn estimate_prompt_tokens(&self) -> i32 {
        estimate_input_tokens(&self.input).max(1)
    }
}

fn estimate_input_tokens(value: &serde_json::Value) -> i32 {
    match value {
        serde_json::Value::String(text) => ((text.len() as f64) / 4.0).ceil() as i32,
        serde_json::Value::Array(items) => items.iter().map(estimate_input_tokens).sum(),
        serde_json::Value::Object(map) => map
            .get("text")
            .map(estimate_input_tokens)
            .unwrap_or_default(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderation_request_estimates_tokens_from_nested_input() {
        let request: ModerationRequest = serde_json::from_value(serde_json::json!({
            "input": [
                "hello world",
                {"type": "text", "text": "unsafe content maybe"}
            ]
        }))
        .unwrap();

        assert_eq!(request.effective_model(), "omni-moderation-latest");
        assert_eq!(request.estimate_prompt_tokens(), 8);
    }

    #[test]
    fn moderation_response_deserializes() {
        let response: ModerationResponse = serde_json::from_value(serde_json::json!({
            "id": "modr-123",
            "model": "omni-moderation-latest",
            "results": [{
                "flagged": false,
                "categories": {"violence": false},
                "category_scores": {"violence": 0.01}
            }]
        }))
        .unwrap();

        assert_eq!(response.id, "modr-123");
        assert_eq!(response.results.len(), 1);
        assert!(!response.results[0].flagged);
    }
}
