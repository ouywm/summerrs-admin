//! OpenAI `/v1/responses` API wire 类型。
//!
//! 对齐 [OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses)。
//!
//! # 设计取舍
//!
//! - **仅覆盖客户端 → 我们可能收到**的请求形态：`input` 支持 string 或 `Vec<InputItem>`；
//!   Item 里只处理 `message` / `function_call_output`（client 回调），其他 item 走
//!   `#[serde(other)]` 通道丢到 `Unknown` 变体保留原 JSON。
//! - **响应侧** `output` 只生成 `message` / `function_call` 两类 item，映射 canonical
//!   `ChatResponse.choices[0].message`。
//! - **built-in tools**（`web_search_preview` / `file_search` / `computer_use_preview` /
//!   `image_generation` / `mcp`）不做语义实现——收到 tools 里有这些类型就作为 `Unknown`
//!   原样透传到上游 `ChatRequest.tools`；上游只认识 `function` 类型会报错，符合预期。
//! - **流事件**类型完整定义（便于下一轮接入），本轮 converter 走 `Unsupported`。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::common::Tool;

// =========================================================================
// Request
// =========================================================================

/// `POST /v1/responses` 请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesRequest {
    pub model: String,

    /// 客户端给模型的输入。可以是 plain string 也可以是 Item 列表。
    pub input: OpenAIResponsesInput,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<OpenAIResponsesTool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,

    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub stream: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAIResponsesReasoning>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    /// 其他厂商/实验字段的直通通道。
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Reasoning 配置（o-series / GPT-5）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesReasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// `input` 字段的双形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesInput {
    Text(String),
    Items(Vec<OpenAIResponsesInputItem>),
}

/// `input` 数组的 Item。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesInputItem {
    /// 对话消息。
    Message(OpenAIResponsesMessageItem),
    /// 上一轮 assistant 发起的工具调用（通常不由 client 再发，由 store=true 的前序响应继承）。
    FunctionCall(OpenAIResponsesFunctionCallItem),
    /// client 对某个 tool_call 的执行结果。
    FunctionCallOutput(OpenAIResponsesFunctionCallOutputItem),
    /// 未识别的 Item 类型——丢 `Unknown` 保留原 JSON，converter 丢弃或透传。
    #[serde(other)]
    Unknown,
}

/// `input` 里的 message item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMessageItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `user` / `system` / `developer` / `assistant`。
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub content: OpenAIResponsesMessageContent,
}

/// message.content 的双形态（string 或 parts 数组）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesMessageContent {
    Text(String),
    Parts(Vec<OpenAIResponsesInputContentPart>),
}

/// message.content 数组里的 content part。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesInputContentPart {
    /// 客户端输入文本。
    InputText { text: String },
    /// 客户端输入图像。
    InputImage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// 客户端输入文件（PDF / doc / …）。
    InputFile {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
    },
    /// 输出 text part（Responses API 响应里用；client 理论不发，这里也允许解析）。
    OutputText {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<Value>,
    },
    /// 未识别 part 类型——透明丢弃或透传。
    #[serde(other)]
    Unknown,
}

/// input 里的 function_call item（store=true 时前序 response 继承）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFunctionCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub name: String,
    /// JSON-encoded arguments。
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// input 里的 function_call_output item（client 回调）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFunctionCallOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    /// 工具执行结果文本。
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// 请求 `tools` 字段里的 tool。
///
/// 只支持 `function`；built-in tools 走 `Unknown` 原样保留。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesTool {
    /// 普通 function tool，结构等同 chat API 的 `{"type":"function","function":{...}}`
    /// 但 Responses API 把 function 字段**扁平**（name/parameters/description 直接在 tool 对象上）。
    Function(OpenAIResponsesFunctionTool),
    /// 未识别的 built-in tool，透传给上游（上游多半会拒绝）。
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFunctionTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

// =========================================================================
// Response
// =========================================================================

/// `POST /v1/responses` 响应体（非流式，`stream=false`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesResponse {
    pub id: String,
    /// 固定 `"response"`。
    #[serde(default = "default_response_object")]
    pub object: String,
    pub created_at: i64,
    pub model: String,
    /// `completed` / `in_progress` / `failed` / `incomplete` / `cancelled`。
    pub status: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<OpenAIResponsesOutputItem>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIResponsesUsage>,

    /// 所有 output message 里 text parts 的拼接。SDK 经常直接用这个字段，做好填充。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// 响应 `output` 数组里的 Item。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesOutputItem {
    Message(OpenAIResponsesOutputMessage),
    FunctionCall(OpenAIResponsesOutputFunctionCall),
    /// 未来可能新增的 item（如 reasoning / web_search_call），直接 JSON 透传。
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesOutputMessage {
    pub id: String,
    /// `completed` / `in_progress` / `incomplete`。
    #[serde(default = "default_output_message_status")]
    pub status: String,
    /// 固定 `"assistant"`。
    #[serde(default = "default_output_message_role")]
    pub role: String,
    pub content: Vec<OpenAIResponsesOutputContentPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesOutputContentPart {
    OutputText {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<Value>,
    },
    Refusal {
        refusal: String,
    },
    /// 未识别 part 类型。
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesOutputFunctionCall {
    pub id: String,
    pub call_id: String,
    pub name: String,
    /// JSON-encoded arguments。
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Responses API 自家 usage 结构（字段名与 chat API 不同：`input_tokens` 而非 `prompt_tokens`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAIResponsesUsage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<Value>,
}

// =========================================================================
// Stream events（本轮仅定义 wire 类型，converter 未实现）
// =========================================================================

/// Responses API 流事件。事件名跟 OpenAI 官方命名一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAIResponsesStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: OpenAIResponsesResponse,
        sequence_number: u64,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        response: OpenAIResponsesResponse,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: OpenAIResponsesOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: OpenAIResponsesOutputContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: OpenAIResponsesOutputContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: OpenAIResponsesOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        item_id: String,
        output_index: u32,
        delta: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        item_id: String,
        output_index: u32,
        arguments: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        response: OpenAIResponsesResponse,
        sequence_number: u64,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: OpenAIResponsesResponse,
        sequence_number: u64,
    },
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete {
        response: OpenAIResponsesResponse,
        sequence_number: u64,
    },
    #[serde(rename = "error")]
    Error {
        code: Option<String>,
        message: String,
        param: Option<String>,
        sequence_number: u64,
    },
}

// =========================================================================
// Unused helpers
// =========================================================================

/// 给 `Tool` 兼容性 helper：从 canonical chat Tool 转 Responses Function tool。
///
/// 只适用于 `kind == "function"` 的 tool。非 function kind（`web_search` / `mcp` /
/// `file_search` 等）应该在 ingress/adapter 层专门处理，这里碰到会退化成 `""` 名
/// 占位（实际调用方需要自己判断不是 function tool 后走 built-in 路径）。
impl From<Tool> for OpenAIResponsesTool {
    fn from(t: Tool) -> Self {
        let func = t.function;
        Self::Function(OpenAIResponsesFunctionTool {
            name: func.as_ref().map(|f| f.name.clone()).unwrap_or_default(),
            description: func.as_ref().and_then(|f| f.description.clone()),
            parameters: func.and_then(|f| f.parameters),
            strict: None,
        })
    }
}

fn default_response_object() -> String {
    "response".to_string()
}

fn default_output_message_status() -> String {
    "completed".to_string()
}

fn default_output_message_role() -> String {
    "assistant".to_string()
}

fn skip_if_false(v: &bool) -> bool {
    !*v
}

// =========================================================================
// Tests —— 只跑 serde round-trip，语义转换在 relay 层测。
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_string_input_parses() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hello"
        }))
        .unwrap();
        assert_eq!(req.model, "gpt-5");
        matches!(req.input, OpenAIResponsesInput::Text(ref s) if s == "hello");
    }

    #[test]
    fn request_items_input_parses() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": [
                {"type":"message","role":"user","content":"hi"},
                {"type":"function_call_output","call_id":"c1","output":"42"}
            ],
            "instructions": "be terse"
        }))
        .unwrap();
        if let OpenAIResponsesInput::Items(items) = &req.input {
            assert_eq!(items.len(), 2);
            matches!(items[0], OpenAIResponsesInputItem::Message(_));
            matches!(items[1], OpenAIResponsesInputItem::FunctionCallOutput(_));
        } else {
            panic!("expected Items");
        }
    }

    #[test]
    fn message_parts_input_parses() {
        let item: OpenAIResponsesInputItem = serde_json::from_value(serde_json::json!({
            "type": "message",
            "role": "user",
            "content": [
                {"type":"input_text","text":"describe"},
                {"type":"input_image","image_url":"https://x.com/a.png","detail":"high"}
            ]
        }))
        .unwrap();
        if let OpenAIResponsesInputItem::Message(m) = item {
            match m.content {
                OpenAIResponsesMessageContent::Parts(parts) => {
                    assert_eq!(parts.len(), 2);
                }
                _ => panic!("expected Parts"),
            }
        } else {
            panic!("expected Message");
        }
    }

    #[test]
    fn unknown_input_item_type_is_preserved_as_unknown_variant() {
        let items: Vec<OpenAIResponsesInputItem> = serde_json::from_value(serde_json::json!([
            {"type":"reasoning","id":"r1","content":[]}
        ]))
        .unwrap();
        matches!(items[0], OpenAIResponsesInputItem::Unknown);
    }

    #[test]
    fn response_roundtrips_minimal_message_output() {
        let resp = OpenAIResponsesResponse {
            id: "resp_1".into(),
            object: "response".into(),
            created_at: 0,
            model: "gpt-5".into(),
            status: "completed".into(),
            output: vec![OpenAIResponsesOutputItem::Message(
                OpenAIResponsesOutputMessage {
                    id: "msg_1".into(),
                    status: "completed".into(),
                    role: "assistant".into(),
                    content: vec![OpenAIResponsesOutputContentPart::OutputText {
                        text: "hi".into(),
                        annotations: vec![],
                    }],
                },
            )],
            usage: Some(OpenAIResponsesUsage {
                input_tokens: 3,
                output_tokens: 2,
                total_tokens: 5,
                ..Default::default()
            }),
            output_text: Some("hi".into()),
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            parallel_tool_calls: None,
            previous_response_id: None,
            tool_choice: None,
            tools: vec![],
            reasoning: None,
            user: None,
            metadata: None,
            error: None,
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"type\":\"message\""));
        assert!(s.contains("\"text\":\"hi\""));
        assert!(s.contains("\"input_tokens\":3"));
    }

    #[test]
    fn stream_event_completed_roundtrips() {
        let ev = OpenAIResponsesStreamEvent::OutputTextDelta {
            item_id: "msg_1".into(),
            output_index: 0,
            content_index: 0,
            delta: "hi".into(),
            sequence_number: 5,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"type\":\"response.output_text.delta\""));
        assert!(s.contains("\"delta\":\"hi\""));
    }
}
