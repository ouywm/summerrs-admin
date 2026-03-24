use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Token 用量统计
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cached_tokens: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reasoning_tokens: i32,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

/// 完成原因
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tool {
    pub r#type: String,
    pub function: FunctionDef,
}

/// 函数定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// 流式工具调用增量
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallDelta {
    pub index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCallDelta>,
}

/// 流式函数调用增量
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// 流式选项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StreamOptions {
    #[serde(default)]
    pub include_usage: Option<bool>,
}

/// 流式消息增量
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trip() {
        let msg = Message {
            role: "user".into(),
            content: serde_json::Value::String("hello".into()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "user");
        assert_eq!(parsed.content, serde_json::Value::String("hello".into()));
    }

    #[test]
    fn usage_zero_fields_skipped() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(!json.contains("cached_tokens"));
        assert!(!json.contains("reasoning_tokens"));
    }

    #[test]
    fn usage_nonzero_fields_present() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            cached_tokens: 5,
            reasoning_tokens: 8,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("cached_tokens"));
        assert!(json.contains("reasoning_tokens"));
    }

    #[test]
    fn usage_deserialize_missing_optional() {
        let json = r#"{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.cached_tokens, 0);
        assert_eq!(usage.reasoning_tokens, 0);
    }

    #[test]
    fn finish_reason_snake_case() {
        let json = serde_json::to_string(&FinishReason::ToolCalls).unwrap();
        assert_eq!(json, r#""tool_calls""#);

        let parsed: FinishReason = serde_json::from_str(r#""content_filter""#).unwrap();
        assert!(matches!(parsed, FinishReason::ContentFilter));
    }

    #[test]
    fn tool_call_round_trip() {
        let tc = ToolCall {
            id: "call_123".into(),
            r#type: "function".into(),
            function: FunctionCall {
                name: "get_weather".into(),
                arguments: r#"{"city":"Beijing"}"#.into(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.function.name, "get_weather");
    }

    #[test]
    fn delta_skip_none_fields() {
        let delta = Delta {
            role: Some("assistant".into()),
            content: None,
            reasoning_content: None,
            tool_calls: None,
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("role"));
        assert!(!json.contains("content"));
        assert!(!json.contains("tool_calls"));
    }
}
