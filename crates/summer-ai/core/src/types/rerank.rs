use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::common::Usage;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RerankRequest {
    pub model: String,
    pub query: String,
    pub documents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_n: Option<i32>,
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RerankResult {
    pub index: usize,
    pub relevance_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RerankResponse {
    #[serde(default)]
    pub results: Vec<RerankResult>,
    #[serde(default)]
    pub usage: Usage,
}
