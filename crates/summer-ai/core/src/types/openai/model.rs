//! `GET /v1/models` 的 canonical response 类型。
//!
//! 与 OpenAI [Models List](https://platform.openai.com/docs/api-reference/models/list)
//! 格式一致：`{"object": "list", "data": [ModelInfo, ...]}`。

use serde::{Deserialize, Serialize};

/// 可用模型列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelList {
    #[serde(default = "default_list_object")]
    pub object: String,
    pub data: Vec<ModelInfo>,
}

impl ModelList {
    pub fn new(data: Vec<ModelInfo>) -> Self {
        Self {
            object: default_list_object(),
            data,
        }
    }
}

/// 单个模型元信息（与 OpenAI 官方 `/v1/models` 响应对齐）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 模型 id（客户端请求时传入的 `model` 字段）
    pub id: String,
    #[serde(default = "default_model_object")]
    pub object: String,
    /// 模型上线时间戳（Unix 秒）
    #[serde(default)]
    pub created: i64,
    /// 拥有方（OpenAI 官方使用 `"openai"` / `"system"` 等）
    #[serde(default = "default_owned_by")]
    pub owned_by: String,
}

fn default_list_object() -> String {
    "list".to_string()
}

fn default_model_object() -> String {
    "model".to_string()
}

fn default_owned_by() -> String {
    "unknown".to_string()
}
