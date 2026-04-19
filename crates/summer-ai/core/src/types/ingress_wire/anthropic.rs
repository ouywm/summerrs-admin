//! Anthropic Messages API 的 wire 类型定义。
//!
//! 严格对齐 [Anthropic Messages API](https://docs.anthropic.com/en/api/messages)
//! 和 [Streaming](https://docs.anthropic.com/en/api/messages-streaming)。
//!
//! # 设计原则
//!
//! - **纯 struct + serde**，无转换逻辑——converter 在 `relay/src/convert/` 实现
//! - 字段用 `Option<T>` + `skip_serializing_if = "Option::is_none"` 保证
//!   "缺省"和"null"语义可区分
//! - `Vec<T>` 用 `skip_serializing_if = "Vec::is_empty"`，空数组不发送
//! - 枚举用 `#[serde(tag = "type", rename_all = "snake_case")]` 匹配 Anthropic
//!   的 `{"type":"text", ...}` 风格
//!
//! # 6 种流事件
//!
//! `message_start` → (`content_block_start` → `content_block_delta*` → `content_block_stop`)+
//! → `message_delta` → `message_stop`
//!
//! 另有 `ping`（保活）和 `error`（流中错误）。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// `POST /v1/messages` 请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    /// Anthropic 必填字段（不像 OpenAI 可选）。
    pub max_tokens: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<AnthropicSystem>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,

    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub stream: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AnthropicTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<AnthropicToolChoice>,

    /// Extended thinking（claude-3.7 / claude-4 系列支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AnthropicMetadata>,

    /// 透传私有 / 未覆盖字段。
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `system` 字段可以是字符串或多块（均可带 `cache_control`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicSystem {
    Text(String),
    Blocks(Vec<AnthropicSystemBlock>),
}

/// `system` 数组形态的元素（只含 text + cache_control）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicSystemBlock {
    #[serde(rename = "type")]
    pub kind: String, // 固定 "text"
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Anthropic 消息（user / assistant）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    /// `"user"` | `"assistant"`
    pub role: String,
    pub content: AnthropicContent,
}

/// 消息 content：字符串 or 多块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

/// Anthropic content block 的全部类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicContentBlock {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        source: AnthropicImageSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<AnthropicToolResultContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Extended thinking 返回的 block（assistant role）。
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// 被 Anthropic 过滤的 thinking（只剩加密 data）。
    RedactedThinking { data: String },
    /// PDF / 文档输入。source 结构多样，先用 Value 兜底。
    Document {
        source: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

/// 图像 source：base64 或 URL。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// tool_result 的 content：字符串或多块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicToolResultContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

/// Prompt cache 控制（Anthropic 独有）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    /// 固定 `"ephemeral"`。
    #[serde(rename = "type")]
    pub kind: String,
    /// `"5m"` | `"1h"`（默认 5m）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

/// 工具声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// `tool_choice` 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicToolChoice {
    Auto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    None,
    Tool {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

/// Extended thinking 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingConfig {
    Enabled {
        budget_tokens: u32,
    },
    Adaptive {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget_tokens: Option<u32>,
    },
    Disabled,
}

/// 用户侧元数据（Anthropic abuse 检测用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Response (non-stream)
// ---------------------------------------------------------------------------

/// 非流式响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicResponse {
    pub id: String,
    #[serde(rename = "type", default = "default_message_type")]
    pub kind: String, // "message"
    pub role: String, // "assistant"
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<AnthropicStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

/// Anthropic 停止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnthropicStopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    Refusal,
    PauseTurn,
}

/// Anthropic usage（含 prompt cache 计费字段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicUsage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    /// 写入 prompt cache 的 token（独立计费，1.25x）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    /// 读命中的 prompt cache token（计费 0.1x）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    /// 5m / 1h 细分（较新 API 返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<AnthropicCacheCreation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

/// Cache creation 5m/1h 细分。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicCacheCreation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral_5m_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral_1h_input_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Stream events (6 种 + ping + error)
// ---------------------------------------------------------------------------

/// Anthropic SSE 事件。
///
/// 正常序列：`message_start` → (`content_block_start` → `content_block_delta*`
/// → `content_block_stop`)+ → `message_delta` → `message_stop`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicStreamMessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: AnthropicStreamContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: AnthropicStreamDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: AnthropicStreamMessageDelta,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<AnthropicUsage>,
    },
    MessageStop,
    /// 保活。客户端忽略即可。
    Ping,
    /// 流中错误。
    Error {
        error: AnthropicErrorBody,
    },
}

/// `message_start` 事件里的 message 对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicStreamMessageStart {
    pub id: String,
    #[serde(rename = "type", default = "default_message_type")]
    pub kind: String,
    pub role: String,
    #[serde(default)]
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<AnthropicStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

/// `content_block_start` 里 `content_block` 字段的 block 类型（不含 cache_control，
/// 流里不会再带 cache hint）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    RedactedThinking {
        data: String,
    },
}

/// `content_block_delta` 里 `delta` 字段的 4 种 delta 类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamDelta {
    TextDelta {
        text: String,
    },
    /// tool_use 的 `arguments` 是 JSON 字符串增量——**Anthropic 官方就是 string**，
    /// 客户端负责累积拼接后反序列化。
    InputJsonDelta {
        partial_json: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    SignatureDelta {
        signature: String,
    },
}

/// `message_delta` 里的 delta 对象（只含 stop_reason / stop_sequence）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicStreamMessageDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<AnthropicStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// `error` 事件的 body。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicErrorBody {
    /// `"invalid_request_error"` / `"overloaded_error"` 等。
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// serde helpers
// ---------------------------------------------------------------------------

fn default_message_type() -> String {
    "message".to_string()
}

fn skip_if_false(v: &bool) -> bool {
    !*v
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_request_roundtrip() {
        let req: AnthropicMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.max_tokens, 64);
        assert_eq!(req.messages.len(), 1);
        assert!(matches!(req.messages[0].content, AnthropicContent::Text(_)));
        assert!(!req.stream);
    }

    #[test]
    fn system_can_be_string_or_array() {
        let s: AnthropicSystem =
            serde_json::from_value(serde_json::json!("you are helpful")).unwrap();
        assert!(matches!(s, AnthropicSystem::Text(_)));

        let b: AnthropicSystem = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "A"},
            {"type": "text", "text": "B", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
        ]))
        .unwrap();
        match b {
            AnthropicSystem::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert!(blocks[1].cache_control.is_some());
            }
            _ => panic!("expected Blocks"),
        }
    }

    #[test]
    fn content_blocks_tool_use_and_result() {
        let blocks: Vec<AnthropicContentBlock> = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "let me check"},
            {"type": "tool_use", "id": "tu_1", "name": "weather", "input": {"city": "NYC"}}
        ]))
        .unwrap();
        assert!(matches!(blocks[0], AnthropicContentBlock::Text { .. }));
        assert!(matches!(blocks[1], AnthropicContentBlock::ToolUse { .. }));

        let result: AnthropicContentBlock = serde_json::from_value(serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "tu_1",
            "content": "72F"
        }))
        .unwrap();
        match result {
            AnthropicContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "tu_1");
                assert!(matches!(content, Some(AnthropicToolResultContent::Text(_))));
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn tool_choice_variants() {
        let auto: AnthropicToolChoice =
            serde_json::from_value(serde_json::json!({"type": "auto"})).unwrap();
        assert!(matches!(auto, AnthropicToolChoice::Auto { .. }));

        let tool: AnthropicToolChoice =
            serde_json::from_value(serde_json::json!({"type": "tool", "name": "weather"})).unwrap();
        match tool {
            AnthropicToolChoice::Tool { name, .. } => assert_eq!(name, "weather"),
            _ => panic!("expected Tool"),
        }
    }

    #[test]
    fn thinking_config_enabled() {
        let t: ThinkingConfig =
            serde_json::from_value(serde_json::json!({"type": "enabled", "budget_tokens": 1024}))
                .unwrap();
        match t {
            ThinkingConfig::Enabled { budget_tokens } => assert_eq!(budget_tokens, 1024),
            _ => panic!("expected Enabled"),
        }
    }

    #[test]
    fn stream_event_message_start() {
        let raw = r#"{
            "type":"message_start",
            "message":{
                "id":"msg_1","type":"message","role":"assistant","content":[],
                "model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,
                "usage":{"input_tokens":5,"output_tokens":0}
            }
        }"#;
        let e: AnthropicStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            AnthropicStreamEvent::MessageStart { message } => {
                assert_eq!(message.id, "msg_1");
                assert_eq!(message.usage.input_tokens, 5);
            }
            _ => panic!("expected MessageStart"),
        }
    }

    #[test]
    fn stream_event_content_block_delta_text() {
        let raw = r#"{
            "type":"content_block_delta",
            "index":0,
            "delta":{"type":"text_delta","text":"hello"}
        }"#;
        let e: AnthropicStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    AnthropicStreamDelta::TextDelta { text } => assert_eq!(text, "hello"),
                    _ => panic!("expected TextDelta"),
                }
            }
            _ => panic!("expected ContentBlockDelta"),
        }
    }

    #[test]
    fn stream_event_input_json_delta() {
        let raw = r#"{
            "type":"content_block_delta",
            "index":1,
            "delta":{"type":"input_json_delta","partial_json":"{\"city\""}
        }"#;
        let e: AnthropicStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            AnthropicStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                AnthropicStreamDelta::InputJsonDelta { partial_json } => {
                    assert!(partial_json.starts_with("{\"city"));
                }
                _ => panic!("expected InputJsonDelta"),
            },
            _ => panic!("expected ContentBlockDelta"),
        }
    }

    #[test]
    fn stream_event_message_delta_with_usage() {
        let raw = r#"{
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn"},
            "usage":{"input_tokens":0,"output_tokens":12}
        }"#;
        let e: AnthropicStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            AnthropicStreamEvent::MessageDelta { delta, usage } => {
                assert_eq!(delta.stop_reason, Some(AnthropicStopReason::EndTurn));
                assert_eq!(usage.unwrap().output_tokens, 12);
            }
            _ => panic!("expected MessageDelta"),
        }
    }

    #[test]
    fn stream_event_message_stop_and_ping() {
        let stop: AnthropicStreamEvent =
            serde_json::from_str(r#"{"type":"message_stop"}"#).unwrap();
        assert!(matches!(stop, AnthropicStreamEvent::MessageStop));

        let ping: AnthropicStreamEvent = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(ping, AnthropicStreamEvent::Ping));
    }

    #[test]
    fn usage_roundtrips_cache_fields() {
        let u: AnthropicUsage = serde_json::from_value(serde_json::json!({
            "input_tokens": 100,
            "output_tokens": 50,
            "cache_creation_input_tokens": 200,
            "cache_read_input_tokens": 80,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 150,
                "ephemeral_1h_input_tokens": 50
            }
        }))
        .unwrap();
        assert_eq!(u.cache_read_input_tokens, Some(80));
        assert_eq!(
            u.cache_creation.as_ref().unwrap().ephemeral_5m_input_tokens,
            Some(150)
        );
    }
}
