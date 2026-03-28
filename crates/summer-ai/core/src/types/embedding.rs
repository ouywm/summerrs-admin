use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// POST /v1/embeddings request body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingsRequest {
    pub model: String,
    pub input: EmbeddingsInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<i32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum EmbeddingsInput {
    Single(String),
    Multiple(Vec<String>),
    SingleTokenIds(Vec<i32>),
    MultipleTokenIds(Vec<Vec<i32>>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingsResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: serde_json::Value,
    pub index: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingUsage {
    pub prompt_tokens: i32,
    pub total_tokens: i32,
}

impl EmbeddingsRequest {
    pub fn estimate_input_tokens(&self) -> i32 {
        match &self.input {
            EmbeddingsInput::Single(text) => estimate_text_tokens(text),
            EmbeddingsInput::Multiple(texts) => {
                texts.iter().map(|text| estimate_text_tokens(text)).sum()
            }
            EmbeddingsInput::SingleTokenIds(token_ids) => token_ids.len() as i32,
            EmbeddingsInput::MultipleTokenIds(token_ids) => {
                token_ids.iter().map(|item| item.len() as i32).sum()
            }
        }
    }
}

fn estimate_text_tokens(text: &str) -> i32 {
    ((text.len() as f64) / 4.0).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddings_request_estimates_text_tokens() {
        let req: EmbeddingsRequest = serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-large",
            "input": ["hello world", "abcd"]
        }))
        .unwrap();

        assert_eq!(req.estimate_input_tokens(), 4);
    }

    #[test]
    fn embeddings_request_estimates_token_ids() {
        let req: EmbeddingsRequest = serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-large",
            "input": [[1, 2, 3], [4, 5]]
        }))
        .unwrap();

        assert_eq!(req.estimate_input_tokens(), 5);
    }
}
