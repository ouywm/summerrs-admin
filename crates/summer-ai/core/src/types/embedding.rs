use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::Usage;

/// POST /v1/embeddings request body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// /v1/embeddings response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub usage: Usage,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingData {
    pub object: String,
    pub index: i32,
    pub embedding: serde_json::Value,
}

pub fn estimate_input_tokens(input: &serde_json::Value) -> i32 {
    ((input.to_string().len() as f64) / 4.0).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_request_round_trip() {
        let req: EmbeddingRequest = serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-large",
            "input": ["hello", "world"],
            "dimensions": 1024,
            "custom_field": "custom"
        }))
        .unwrap();

        assert_eq!(req.model, "text-embedding-3-large");
        assert_eq!(req.dimensions, Some(1024));
        assert_eq!(req.extra.get("custom_field").unwrap(), "custom");
    }

    #[test]
    fn embedding_response_deserialize() {
        let response: EmbeddingResponse = serde_json::from_value(serde_json::json!({
            "object": "list",
            "data": [
                {
                    "object": "embedding",
                    "index": 0,
                    "embedding": [0.1, 0.2]
                }
            ],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 0,
                "total_tokens": 8
            }
        }))
        .unwrap();

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.usage.total_tokens, 8);
    }

    #[test]
    fn estimate_input_tokens_uses_json_size() {
        let input = serde_json::json!(["hello", "world"]);
        assert!(estimate_input_tokens(&input) > 0);
    }
}
