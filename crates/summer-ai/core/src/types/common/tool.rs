//! 工具调用相关类型（function calling）。

use serde::{Deserialize, Serialize};

/// 工具声明（调用方告诉上游"你可以用这个工具"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: ToolFunction,
}

impl Tool {
    pub fn function(function: ToolFunction) -> Self {
        Self {
            kind: "function".to_string(),
            function,
        }
    }
}

/// 工具函数签名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// 上游响应中的"实际发起的工具调用"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON-encoded arguments。上游一般就给个字符串，不反序列化。
    pub arguments: String,
}

/// 调用方告知上游如何挑选工具。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// `"auto"` / `"none"` / `"required"`。
    Mode(String),
    /// `{"type": "function", "function": {"name": "..."}}`。
    Named(serde_json::Value),
}

fn default_tool_type() -> String {
    "function".to_string()
}
