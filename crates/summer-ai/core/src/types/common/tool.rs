//! 工具调用相关类型（function calling）。

use serde::{Deserialize, Serialize};

/// 工具声明（调用方告诉上游"你可以用这个工具"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: ToolFunction,
    /// OpenAI structured output：`strict: true` → 强制 schema 验证 +
    /// 自动注入 `additionalProperties: false` 到每个 object 节点。
    ///
    /// 只有 OpenAI 家族认这个字段；其他 provider adapter 遇到不认的 field
    /// 会 ignore，所以安全序列化。`None` 时 skip 序列化，不会在 wire 里
    /// 写出 `"strict": null` 让某些严格上游拒收。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Tool {
    pub fn function(function: ToolFunction) -> Self {
        Self {
            kind: "function".to_string(),
            function,
            strict: None,
        }
    }

    /// Chainable：开启 structured output 严格模式（仅 OpenAI 认）。
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
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
    /// Gemini 3 thinking 的 `thoughtSignature` —— multi-turn 续接 tool use 时
    /// 客户端必须把这些 signature 在 assistant 消息里放在 `tool_calls` 之前
    /// 回传，上游才能继承思考状态；否则 400。
    ///
    /// 流式场景下 signature 走 `ChatStreamEvent::ThoughtSignature` 独立事件 + 客户端
    /// 自行累积；非流式响应里由 adapter 把 candidate part 上的 signature 收拢到这里。
    ///
    /// OpenAI / Claude 上游无对应字段，skip 序列化避免出现空字段污染 wire。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signatures: Option<Vec<String>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_strict_skipped_when_none() {
        // strict = None 不能写成 "strict": null —— OpenAI 某些兼容网关
        // 对 null 字段严格 400。必须 skip。
        let t = Tool::function(ToolFunction {
            name: "noop".to_string(),
            description: None,
            parameters: None,
        });
        let v = serde_json::to_value(&t).unwrap();
        assert!(v.get("strict").is_none());
    }

    #[test]
    fn tool_strict_roundtrip_true() {
        let t = Tool::function(ToolFunction {
            name: "get_weather".to_string(),
            description: None,
            parameters: None,
        })
        .with_strict(true);
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["strict"], true);
        let back: Tool = serde_json::from_value(v).unwrap();
        assert_eq!(back.strict, Some(true));
    }

    #[test]
    fn tool_strict_default_none_when_absent() {
        // 反序列化没有 strict 字段的 payload 时 —— 对齐默认：None。
        let raw = r#"{"type":"function","function":{"name":"f"}}"#;
        let t: Tool = serde_json::from_str(raw).unwrap();
        assert_eq!(t.strict, None);
    }
}
