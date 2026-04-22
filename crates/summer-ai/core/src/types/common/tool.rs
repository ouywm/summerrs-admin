//! 工具调用相关类型（function calling + provider 内置工具透传）。
//!
//! # 为什么 `function` 是 `Option`
//!
//! 客户端 tools 字段除了标准 OpenAI function tool（`{type:"function", function:{...}}`）
//! 之外，还可能携带各家的 **provider-native built-in tools**：
//!
//! - OpenAI：`web_search_preview` / `web_search` / `code_interpreter` / `file_search` / `mcp`
//! - Anthropic：`web_search_20250305` / `computer_20241022` / `text_editor_20250728` /
//!   `mcp_connector_20250716`
//! - Gemini：`googleSearch` / `google_search_retrieval` / `url_context`（wire 形态 key-based）
//!
//! 这些工具都 **没有** `function` 字段，如果 canonical 把 `function` 当必填，整个请求
//! 会在反序列化阶段 400 拒收，导致 built-in / MCP 工具完全不可用。
//!
//! 解决：`function: Option`，其余字段通过 `#[serde(flatten)] extra` 原样承载。
//! adapter 按 `kind` 分派：function tool 走强类型路径，built-in 透传或翻译到上游方言。

use serde::{Deserialize, Serialize};

/// 工具声明（调用方告诉上游"你可以用这个工具"）。
///
/// 三类 tool 共用这一个结构：
///
/// ```json
/// // 1) 函数工具（最常见，所有 provider 都认）
/// {"type":"function", "function":{"name":"weather", "parameters":{...}}}
///
/// // 2) OpenAI 内置（OpenAI chat.completions / Responses）
/// {"type":"web_search_preview"}
/// {"type":"mcp", "server_label":"brave", "server_url":"https://..."}
///
/// // 3) Anthropic 内置（/v1/messages）
/// {"type":"web_search_20250305", "max_uses":5, "allowed_domains":["..."]}
/// ```
///
/// 结构差异：
/// - `kind == "function"` → 读 `function`
/// - 其他 kind → `function` 为 None，额外字段（`max_uses` / `server_url` / ...）
///   进 `extra`（`#[serde(flatten)]`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// 工具类型。`function` 为默认；其他值表示 provider-native built-in。
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,

    /// Function tool 的签名。只在 `kind == "function"` 时有意义；其他 built-in
    /// 工具此字段为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolFunction>,

    /// OpenAI structured output：`strict: true` → 强制 schema 验证 +
    /// 自动注入 `additionalProperties: false` 到每个 object 节点。
    ///
    /// 只有 OpenAI 家族认这个字段；其他 provider adapter 遇到不认的 field
    /// 会 ignore，所以安全序列化。`None` 时 skip 序列化，不会在 wire 里
    /// 写出 `"strict": null` 让某些严格上游拒收。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    /// 非 function kind 的所有其余字段（`max_uses` / `server_url` / `allowed_domains`
    /// / `name`（Anthropic built-in 需要）/ `server_label` / `authorization_token`
    /// 等）。canonical 层不枚举每种 built-in，透明承载让 adapter 按需取用 + 原样
    /// 写回 wire。`#[serde(flatten)]` 保证未知字段双向不丢失。
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Tool {
    /// 构造 function tool。`kind` 自动设为 `"function"`。
    pub fn function(function: ToolFunction) -> Self {
        Self {
            kind: "function".to_string(),
            function: Some(function),
            strict: None,
            extra: serde_json::Map::new(),
        }
    }

    /// 构造 built-in / 其他类型的 tool。`kind` 是 `"web_search_preview"` /
    /// `"mcp"` / `"web_search_20250305"` 等上游认的 type 字符串；所有载荷字段
    /// 都放进 `extra`。
    pub fn builtin(
        kind: impl Into<String>,
        extra: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        Self {
            kind: kind.into(),
            function: None,
            strict: None,
            extra,
        }
    }

    /// Chainable：开启 structured output 严格模式（仅 OpenAI 认）。
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
    }

    /// 是否是普通的 function tool。`kind != "function"` 一律当作 provider-native
    /// built-in。adapter 用这个判断是否走翻译路径。
    pub fn is_function(&self) -> bool {
        self.kind == "function"
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
    use serde_json::json;

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

    #[test]
    fn tool_without_function_field_deserializes_as_builtin() {
        // 阻塞 bug 修复：客户端传 `{"type":"web_search_preview"}` 过去反序列化
        // 会要求 function 字段 → 400。现在应该顺利建成 kind="web_search_preview"、
        // function=None、extra={}。
        let raw = r#"{"type":"web_search_preview"}"#;
        let t: Tool = serde_json::from_str(raw).unwrap();
        assert_eq!(t.kind, "web_search_preview");
        assert!(t.function.is_none());
        assert!(t.extra.is_empty());
        assert!(!t.is_function());
    }

    #[test]
    fn tool_mcp_preserves_server_fields_in_extra() {
        // MCP connector：客户端传的 server_label / server_url / authorization_token
        // 必须原样落到 extra，adapter 才能透传到上游（OpenAI Responses mcp tool
        // 或 Anthropic mcp_connector_20250716）。
        let raw = r#"{
            "type":"mcp",
            "server_label":"brave",
            "server_url":"https://api.brave.example/mcp",
            "authorization_token":"sk-xxx",
            "allowed_tools":["search","fetch"]
        }"#;
        let t: Tool = serde_json::from_str(raw).unwrap();
        assert_eq!(t.kind, "mcp");
        assert!(t.function.is_none());
        assert_eq!(t.extra["server_label"], json!("brave"));
        assert_eq!(
            t.extra["server_url"],
            json!("https://api.brave.example/mcp")
        );
        assert_eq!(t.extra["authorization_token"], json!("sk-xxx"));
        assert_eq!(t.extra["allowed_tools"], json!(["search", "fetch"]));
    }

    #[test]
    fn tool_anthropic_web_search_preserves_config() {
        // Anthropic 的 web_search_20250305 带 max_uses / allowed_domains；
        // 这些都走 extra，adapter 发送时原样组装回去。
        let raw = r#"{
            "type":"web_search_20250305",
            "name":"web_search",
            "max_uses":5,
            "allowed_domains":["example.com","docs.example.com"]
        }"#;
        let t: Tool = serde_json::from_str(raw).unwrap();
        assert_eq!(t.kind, "web_search_20250305");
        assert!(t.function.is_none());
        assert_eq!(t.extra["name"], json!("web_search"));
        assert_eq!(t.extra["max_uses"], json!(5));
        assert_eq!(
            t.extra["allowed_domains"],
            json!(["example.com", "docs.example.com"])
        );
    }

    #[test]
    fn tool_function_roundtrip_no_extra_leak() {
        // function tool 序列化时 extra 为空 → 不能输出 `"extra":{}` 污染 wire。
        // `#[serde(flatten)]` + `skip_serializing_if` 共同保证。
        let t = Tool::function(ToolFunction {
            name: "weather".to_string(),
            description: Some("Get weather".to_string()),
            parameters: Some(json!({"type":"object"})),
        });
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["type"], "function");
        assert!(v["function"].is_object());
        assert!(v.get("extra").is_none());
        // Roundtrip 回来不丢字段。
        let back: Tool = serde_json::from_value(v).unwrap();
        assert_eq!(back.kind, "function");
        assert_eq!(back.function.as_ref().unwrap().name, "weather");
    }

    #[test]
    fn tool_builtin_roundtrip_preserves_all_fields() {
        // 构造 → JSON → 反序列化 三轮后，extra 里所有字段必须保持原位。
        let mut extra = serde_json::Map::new();
        extra.insert("server_label".to_string(), json!("custom_mcp"));
        extra.insert("server_url".to_string(), json!("https://example.com"));
        let t = Tool::builtin("mcp", extra);
        let v = serde_json::to_value(&t).unwrap();
        // `type` 在根层，server_* 也平铺在根层 —— 这是 wire 契约。
        assert_eq!(v["type"], "mcp");
        assert_eq!(v["server_label"], json!("custom_mcp"));
        assert_eq!(v["server_url"], json!("https://example.com"));
        assert!(v.get("function").is_none());
        let back: Tool = serde_json::from_value(v).unwrap();
        assert_eq!(back.kind, "mcp");
        assert_eq!(back.extra["server_label"], json!("custom_mcp"));
    }
}
