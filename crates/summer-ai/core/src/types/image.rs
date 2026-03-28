use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageGenerationRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageGenerationResponse {
    pub created: i64,
    pub data: Vec<ImageGenerationData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageGenerationData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b64_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
}

impl ImageGenerationRequest {
    pub fn estimate_prompt_tokens(&self) -> i32 {
        ((self.prompt.len() as f64) / 4.0).ceil() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_generation_request_estimates_tokens() {
        let req: ImageGenerationRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-image-1",
            "prompt": "draw a small red fox",
            "size": "1024x1024"
        }))
        .unwrap();

        assert_eq!(req.estimate_prompt_tokens(), 5);
    }

    #[test]
    fn image_generation_response_deserializes() {
        let response: ImageGenerationResponse = serde_json::from_value(serde_json::json!({
            "created": 1700000000,
            "data": [{"url": "https://example.com/image.png"}]
        }))
        .unwrap();

        assert_eq!(response.data.len(), 1);
        assert_eq!(
            response.data[0].url.as_deref(),
            Some("https://example.com/image.png")
        );
    }
}
