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

/// 一条 chat message。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    /// assistant 仅返回 tool_calls / audio 时 content 可为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
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
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            audio: None,
        }
    }

    fn of(role: Role, content: MessageContent) -> Self {
        Self {
            role,
            content: Some(content),
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
        }
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
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
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
}
