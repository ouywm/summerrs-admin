//! OpenAI Chat Completions 请求/响应的 wire 类型。
//!
//! 字段 **严格对齐** [OpenAI 官方 API](https://platform.openai.com/docs/api-reference/chat/create)。
//! 共享类型（`ChatMessage` / `Usage` / `FinishReason` / `Tool`）从 `types/common/` 引入。
//!
//! # 字段对照
//!
//! 每个字段的含义、默认值、取值范围，都可以直接到上述官方文档搜索对应 key。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::common::{
    ChatMessage, FinishReason, ReasoningEffort, ServiceTier, Tool, ToolChoice, Usage, Verbosity,
    WebSearchOptions,
};

/// `POST /v1/chat/completions` 请求体。字段顺序按官方文档列出。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatRequest {
    // ------------------------------- 必选 -------------------------------
    /// 上游模型名（Relay 里是"逻辑模型"，会被 `channel.model_mapping` 再映射）。
    pub model: String,
    /// 对话历史。
    pub messages: Vec<ChatMessage>,

    // ------------------------------ 生成控制 -----------------------------
    /// 采样温度（0..=2，默认 1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// nucleus sampling，默认 1。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// 生成候选数，默认 1。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    /// 停止 token / 序列。可以是 string、string 数组，也可以是 null。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,

    // --------------------------- 长度 / 预算控制 -------------------------
    /// 老参数，新模型（o1/GPT-4o-2024-11 之后）推荐用 `max_completion_tokens`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    /// 替代 `max_tokens`。o1 系列**只认**这个。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i64>,

    // ---------------------------- 惩罚 / 偏置 ----------------------------
    /// 频次惩罚，-2.0..=2.0，默认 0。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    /// 存在惩罚，-2.0..=2.0，默认 0。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    /// token_id (string) → bias (-100..=100)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, f64>>,

    // ---------------------------- Logprobs ------------------------------
    /// 是否返回 logprobs。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// 返回最可能的前 N 个 token 的 logprob（0..=20）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,

    // ---------------------------- 输出格式 ------------------------------
    /// `{"type": "text" | "json_object" | "json_schema"}`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// `["text"]` / `["text","audio"]`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    /// 音频输出参数（需在 `modalities` 里包含 `"audio"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioOutputOptions>,
    /// Predicted outputs（推测解码加速）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction: Option<serde_json::Value>,

    // ---------------------------- Tools --------------------------------
    /// 可用工具列表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// `"auto"` / `"none"` / `"required"` / `{"type":"function", ...}`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 是否允许并行调用多个工具（默认 true）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    // ---------------------------- 流式 --------------------------------
    /// 是否 SSE 流式返回（默认 false）。
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub stream: bool,
    /// `{"include_usage": true}` 让流式响应末尾多一个带 usage 的 chunk。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,

    // --------------------------- 推理 / 其它 ---------------------------
    /// o-series / GPT-5：`minimal` / `low` / `medium` / `high`，或上游特定的
    /// token budget（Anthropic thinking / Gemini thinkingBudget 数字形态）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// GPT-5 系列的回答详尽度提示（`low` / `medium` / `high`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    /// 固定种子用于 deterministic（best-effort，不保证 100% 复现）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// OpenAI 服务等级偏好（`auto` / `default` / `flex` / `priority` / `scale`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,

    // ------------------------- 账户 / 审计 -----------------------------
    /// 用户标识（OpenAI abuse 检测用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 开发者自定义元数据（<= 16 个 key-value）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// 是否把本次对话存进 OpenAI（用于后续训练 / distillation，默认 false）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Web search 工具参数（GPT-4o-search-preview 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search_options: Option<WebSearchOptions>,

    // ---------------------- 未来新增字段的兜底透传 ----------------------
    /// OpenAI 未来可能加新字段；同时也用来透传第三方 compat 厂商的私有字段。
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            ..Default::default()
        }
    }
}

// -------------------------------- Response --------------------------------

/// 非流式响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    #[serde(default = "default_response_object")]
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Usage,
    /// OpenAI 的指纹，透传用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// 上游实际使用的 service_tier（对应请求里的 `service_tier` 字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
}

impl ChatResponse {
    /// 取第一个 choice 的文本（便利方法；多 choice 场景遍历 `choices`）。
    pub fn first_text(&self) -> Option<&str> {
        self.choices.first().and_then(|c| c.message.text())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: i32,
    pub message: ChatMessage,
    /// `logprobs` 响应对象；OpenAI 没开 logprobs 时是 null。结构较复杂，先用 Value。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
}

// --------------------------- Response format ------------------------------

/// `response_format` 字段的三种形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// `{"type": "text"}`
    Text,
    /// `{"type": "json_object"}`
    JsonObject,
    /// `{"type": "json_schema", "json_schema": {...}}`
    JsonSchema { json_schema: JsonSchemaFormat },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchemaFormat {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

// ---------------------------- Stream options ------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

// ----------------------------- Audio output -------------------------------

/// 请求里的 `audio` 字段（客户端告诉上游想要什么 voice / format）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioOutputOptions {
    /// `alloy` / `echo` / `fable` / `onyx` / `nova` / `shimmer` ...
    pub voice: String,
    /// `mp3` / `opus` / `aac` / `flac` / `wav` / `pcm16`。
    pub format: String,
}

// ------------------------ serde helper ------------------------------------

fn default_response_object() -> String {
    "chat.completion".to_string()
}

fn skip_if_false(v: &bool) -> bool {
    !*v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_minimal_roundtrip() {
        let req = ChatRequest::new("gpt-4o-mini", vec![ChatMessage::user("hello")]);
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["model"], "gpt-4o-mini");
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
        // stream 默认 false 时应被省略（skip_if_false）
        assert!(value.get("stream").is_none());
        assert!(value.get("temperature").is_none());
        assert!(value.get("max_tokens").is_none());
    }

    #[test]
    fn request_unknown_fields_preserved_via_flatten_extra() {
        let req: ChatRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hi"}],
            "custom_vendor_x": true
        }))
        .unwrap();
        assert_eq!(req.extra["custom_vendor_x"], true);
    }

    #[test]
    fn request_flat_fields_match_openai_shape() {
        let req: ChatRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 0.7,
            "top_p": 0.9,
            "n": 1,
            "max_completion_tokens": 1024,
            "frequency_penalty": 0.1,
            "presence_penalty": 0.2,
            "logit_bias": {"50256": -100.0},
            "logprobs": true,
            "top_logprobs": 5,
            "seed": 42,
            "user": "u-123",
            "service_tier": "auto",
            "parallel_tool_calls": false,
            "reasoning_effort": "medium",
            "store": true,
            "metadata": {"campaign": "launch"}
        }))
        .unwrap();
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.top_p, Some(0.9));
        assert_eq!(req.max_completion_tokens, Some(1024));
        assert_eq!(req.logit_bias.as_ref().unwrap()["50256"], -100.0);
        assert_eq!(req.top_logprobs, Some(5));
        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::Medium));
        assert_eq!(req.user.as_deref(), Some("u-123"));
    }

    #[test]
    fn response_format_json_schema_roundtrip() {
        let fmt = ResponseFormat::JsonSchema {
            json_schema: JsonSchemaFormat {
                name: "Person".to_string(),
                description: None,
                schema: serde_json::json!({"type": "object"}),
                strict: Some(true),
            },
        };
        let v = serde_json::to_value(&fmt).unwrap();
        assert_eq!(v["type"], "json_schema");
        assert_eq!(v["json_schema"]["name"], "Person");
        assert_eq!(v["json_schema"]["strict"], true);

        let back: ResponseFormat = serde_json::from_value(v).unwrap();
        assert!(matches!(back, ResponseFormat::JsonSchema { .. }));
    }

    #[test]
    fn response_format_text_and_json_object() {
        let text: ResponseFormat =
            serde_json::from_value(serde_json::json!({"type": "text"})).unwrap();
        assert!(matches!(text, ResponseFormat::Text));

        let json_object: ResponseFormat =
            serde_json::from_value(serde_json::json!({"type": "json_object"})).unwrap();
        assert!(matches!(json_object, ResponseFormat::JsonObject));
    }
}
