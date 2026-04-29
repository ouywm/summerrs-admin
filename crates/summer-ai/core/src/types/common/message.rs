//! 消息 / 角色 / 多模态内容。
//!
//! 字段对齐 [OpenAI Message 对象](https://platform.openai.com/docs/api-reference/chat/object#chat/object-choices)。

use serde::{Deserialize, Serialize};

use super::tool::ToolCall;

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    /// `developer` 角色（OpenAI 较新模型用来替代 `system` 的叫法）。
    #[serde(rename = "developer")]
    Developer,
}

/// Prompt cache 意图（跨 provider 抽象）。
///
/// 上游映射：
/// - Anthropic：挂到 content block 的 `cache_control` 字段；`Ephemeral` /
///   `Ephemeral5m` → `{type:"ephemeral", ttl:"5m"}`，`Ephemeral1h` →
///   `{type:"ephemeral", ttl:"1h"}`；`Memory` / `Ephemeral24h` 目前 fallback 到 5m
///   （Anthropic wire 暂只支持 5m/1h 两档）。
/// - OpenAI：request-level 的 `prompt_cache_key` 只影响全局缓存命中；per-message
///   cache_control 在 OpenAI wire 无对应字段，adapter 忽略。
/// - Gemini：`explicit cache` / `implicit cache` 走不同 API，per-message 标记目前
///   不映射（等 Gemini 扩字段再补）。
///
/// TTL 排序约束（Anthropic）：同请求混用不同 TTL 时，**长 TTL 必须在短 TTL 之前**。
/// `Ephemeral1h` 条目必须排在任何 `Ephemeral` / `Memory` / `Ephemeral5m` 之前，
/// 否则上游会 reject。canonical 层不校验，调用者自行保证。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheControl {
    /// 默认 ephemeral（5m TTL）。
    Ephemeral,
    /// Memory cache（上游支持则用；Anthropic fallback 到 ephemeral）。
    Memory,
    /// 显式 5 分钟 TTL。
    Ephemeral5m,
    /// 扩展 1 小时 TTL（Anthropic 2x 价格）。
    Ephemeral1h,
    /// 扩展 24 小时 TTL（部分 provider 扩展，Anthropic 目前 fallback 到 1h）。
    Ephemeral24h,
}

/// 单条消息的 provider-agnostic 选项。
///
/// 现在只承载 [`CacheControl`]；未来如需加 per-message 级别的其他控制
/// （如 Anthropic 的 citation / retrieval flags）也塞这里，避免 `ChatMessage`
/// 被无关可选字段撑胖。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageOptions {
    /// Per-message prompt cache 意图。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// Claude 原生 stop_reason（仅内部透传，不进入 wire）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_stop_reason: Option<String>,
    /// Claude 原生 stop_sequence（仅内部透传，不进入 wire）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_stop_sequence: Option<String>,
}

impl MessageOptions {
    pub fn with_cache_control(mut self, cc: CacheControl) -> Self {
        self.cache_control = Some(cc);
        self
    }
}

impl From<CacheControl> for MessageOptions {
    fn from(cc: CacheControl) -> Self {
        Self {
            cache_control: Some(cc),
            claude_stop_reason: None,
            claude_stop_sequence: None,
        }
    }
}

/// 一条 chat message。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    /// assistant 仅返回 tool_calls / audio 时 content 可为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    /// 思考链内容（非流式响应用）。DeepSeek R1 / Kimi / OpenRouter 上游字段名
    /// 是 `reasoning_content`；OpenAI o1 / Ollama 用 `reasoning`（反序列化时作
    /// fallback，见 serde alias）。Claude 非流式的 `thinking` content block、
    /// Gemini 的 `part.thought=true` 对应的 text 也映射到这里。流式下走
    /// `ChatStreamEvent::ReasoningDelta`，此字段保持 None。
    #[serde(default, alias = "reasoning", skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// o1 / GPT-4o 在拒绝回答时会填 `refusal` 而非 `content`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// tool 响应消息或 function-call 的 name。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// assistant 发起的工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// tool 响应消息必填：对应的 assistant tool_calls.id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// assistant 的音频响应（gpt-4o-audio-preview 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioResponse>,
    /// provider-native 原始 content blocks。
    ///
    /// 当 canonical `content` / `tool_calls` / `reasoning_content` 无法完整表达上游
    /// 的原始 block 语义时（例如 Claude 的 `search_result` / `web_search_tool_result`
    /// / `container_upload` 等），保留整段原始 blocks 供同 provider 往返恢复或后续
    /// 业务层结构化消费。
    ///
    /// 这是内部字段，不进入任何 wire payload。
    #[serde(skip)]
    pub native_content_blocks: Option<serde_json::Value>,
    /// Provider-agnostic 的 per-message 选项（目前是 prompt cache 意图）。
    ///
    /// 不进 OpenAI wire（OpenAI 无对应字段）；Claude ingress/adapter 负责
    /// 映射到 `cache_control` 字段。`#[serde(skip)]` 防止出现在任何 wire payload。
    #[serde(skip)]
    pub options: Option<MessageOptions>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self::of(Role::System, MessageContent::text(text))
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self::of(Role::User, MessageContent::text(text))
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::of(Role::Assistant, MessageContent::text(text))
    }
    pub fn tool_response(tool_call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(MessageContent::text(text)),
            reasoning_content: None,
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            audio: None,
            native_content_blocks: None,
            options: None,
        }
    }

    fn of(role: Role, content: MessageContent) -> Self {
        Self {
            role,
            content: Some(content),
            reasoning_content: None,
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
            native_content_blocks: None,
            options: None,
        }
    }

    /// Chainable：挂一个 provider-agnostic 的 cache 意图到本消息。
    ///
    /// Claude adapter 会把它映射到 wire 的 `cache_control` 字段（挂在该 message
    /// 最后一个 content block 上，Anthropic 推荐做法）。其他 provider 暂忽略。
    pub fn with_cache_control(mut self, cc: CacheControl) -> Self {
        let mut options = self.options.unwrap_or_default();
        options.cache_control = Some(cc);
        self.options = Some(options);
        self
    }

    pub fn with_native_content_blocks(mut self, blocks: serde_json::Value) -> Self {
        self.native_content_blocks = Some(blocks);
        self
    }

    /// 取出第一段文本（多模态场景只返回第一个 Text part）。
    pub fn text(&self) -> Option<&str> {
        match self.content.as_ref()? {
            MessageContent::Text(text) => Some(text.as_str()),
            MessageContent::Parts(parts) => parts.iter().find_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            }),
        }
    }
}

/// 消息内容：纯文本 or 多模态 parts。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本（兼容 OpenAI `"content": "..."`）。
    Text(String),
    /// 多模态 parts（兼容 OpenAI `"content": [{"type":"text",...}, ...]`）。
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }
    pub fn parts(parts: Vec<ContentPart>) -> Self {
        Self::Parts(parts)
    }
}

/// 多模态 content part。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrl,
    },
    /// 输入音频（`gpt-4o-audio-preview` 等）。
    InputAudio {
        input_audio: InputAudio,
    },
}

/// OpenAI 风格的 image_url。`detail` 可为 `"auto"` / `"low"` / `"high"`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 输入音频（base64 + format）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputAudio {
    /// base64 编码的音频数据。
    pub data: String,
    /// `wav` / `mp3` 等。
    pub format: String,
}

/// assistant 返回的音频对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioResponse {
    pub id: String,
    pub expires_at: i64,
    /// base64 编码的音频数据。
    pub data: String,
    /// 转录文本。
    pub transcript: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_variant_roundtrips_as_plain_string() {
        let msg = ChatMessage::user("hello");
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(value["content"], "hello");
        let back: ChatMessage = serde_json::from_value(value).unwrap();
        assert_eq!(back.role, Role::User);
        assert_eq!(back.text(), Some("hello"));
    }

    #[test]
    fn parts_variant_serializes_as_array() {
        let msg = ChatMessage {
            role: Role::User,
            content: Some(MessageContent::parts(vec![
                ContentPart::Text {
                    text: "describe".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: "https://example.com/a.png".to_string(),
                        detail: Some("auto".to_string()),
                    },
                },
            ])),
            reasoning_content: None,
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
            native_content_blocks: None,
            options: None,
        };
        let value = serde_json::to_value(&msg).unwrap();
        assert!(value["content"].is_array());
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(value["content"][1]["type"], "image_url");
    }

    #[test]
    fn tool_response_sets_role_and_id() {
        let msg = ChatMessage::tool_response("call-1", "42");
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn refusal_field_roundtrips() {
        let msg: ChatMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "refusal": "I can't help with that."
        }))
        .unwrap();
        assert_eq!(msg.refusal.as_deref(), Some("I can't help with that."));
        assert!(msg.content.is_none());
    }

    #[test]
    fn reasoning_content_deserializes_from_either_field_name() {
        // DeepSeek R1 / Kimi / OpenRouter 用 `reasoning_content`；OpenAI o1 / Ollama
        // 的某些版本用 `reasoning`。serde alias 让 canonical 一个字段承接两者，
        // 避免 non-stream 响应里思考链被直接吞掉。
        let m1: ChatMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": "answer",
            "reasoning_content": "think A"
        }))
        .unwrap();
        assert_eq!(m1.reasoning_content.as_deref(), Some("think A"));

        let m2: ChatMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": "answer",
            "reasoning": "think B"
        }))
        .unwrap();
        assert_eq!(m2.reasoning_content.as_deref(), Some("think B"));
    }

    #[test]
    fn reasoning_content_none_skipped_in_serialize() {
        // None 时不能序列化成 `"reasoning_content": null`，否则发请求给上游会
        // 被当成字段显式为 null，个别严格上游会 400。
        let msg = ChatMessage::assistant("hello");
        let value = serde_json::to_value(&msg).unwrap();
        assert!(value.get("reasoning_content").is_none());
    }

    #[test]
    fn native_content_blocks_skipped_in_serialize() {
        let msg = ChatMessage::assistant("hello")
            .with_native_content_blocks(serde_json::json!([{"type":"search_result"}]));
        let value = serde_json::to_value(&msg).unwrap();
        assert!(value.get("native_content_blocks").is_none());
    }
}
