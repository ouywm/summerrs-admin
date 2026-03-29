use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::StreamOptions;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: serde_json::Value,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
