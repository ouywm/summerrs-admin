//! OpenAI `/v1/responses` API wire 类型。
//!
//! 对齐 [OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses)。

use std::collections::HashMap;

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::common::{InputAudio, Tool};

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
    pub tool_choice: Option<OpenAIResponsesToolChoice>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,

    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub stream: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<OpenAIResponsesStreamOptions>,

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

    /// 是否在后台运行响应。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,

    /// 上下文管理配置。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_management: Vec<OpenAIResponsesContextManagement>,

    /// 此响应所属的会话，可以是会话 ID 字符串或会话对象。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<OpenAIResponsesConversationParam>,

    /// 指定要包含在响应中的额外输出数据。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,

    /// 引用 prompt 模板及其变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<OpenAIResponsesPrompt>,

    /// 用于缓存的 key。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,

    /// 缓存保留策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<String>,

    /// 安全标识符。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,

    /// 服务层级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    /// 文本响应配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenAIResponsesTextConfig>,

    /// 返回最可能 token 的数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,

    /// 截断策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,

    /// 其他厂商/实验字段的直通通道。
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// 流选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesStreamOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

/// 上下文管理配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesContextManagement {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_threshold: Option<i64>,
}

/// 会话参数，支持直接传 ID 字符串或完整对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesConversationParam {
    Id(String),
    Object(OpenAIResponsesConversation),
}

/// 会话对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesConversation {
    pub id: String,
}

/// Prompt 引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesPrompt {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// 文本响应配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesTextConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<OpenAIResponsesTextFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
}

/// 文本格式配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesTextFormat {
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        schema: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

/// Reasoning 配置（o-series / GPT-5）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesReasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<OpenAIResponsesReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<OpenAIResponsesReasoningSummary>,
    /// 已废弃，使用 summary。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<String>,
}

/// Reasoning effort 枚举。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAIResponsesReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

/// Reasoning summary 枚举。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAIResponsesReasoningSummary {
    Auto,
    Concise,
    Detailed,
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
    /// 上一轮 assistant 发起的工具调用。
    FunctionCall(OpenAIResponsesFunctionCallItem),
    /// client 对某个 tool_call 的执行结果。
    FunctionCallOutput(OpenAIResponsesFunctionCallOutputItem),
    /// Reasoning 内容。
    Reasoning(OpenAIResponsesReasoningItem),
    /// 文件搜索调用。
    FileSearchCall(OpenAIResponsesFileSearchCallItem),
    /// Computer use 调用。
    ComputerCall(OpenAIResponsesComputerCallItem),
    /// Computer use 调用输出。
    ComputerCallOutput(OpenAIResponsesComputerCallOutputItem),
    /// Web 搜索调用。
    WebSearchCall(OpenAIResponsesWebSearchCallItem),
    /// 工具搜索调用。
    ToolSearchCall(OpenAIResponsesToolSearchCallItem),
    /// 工具搜索输出。
    ToolSearchOutput(OpenAIResponsesToolSearchOutputItem),
    /// 图片生成调用。
    ImageGenerationCall(OpenAIResponsesImageGenerationCallItem),
    /// 代码解释器调用。
    CodeInterpreterCall(OpenAIResponsesCodeInterpreterCallItem),
    /// 本地 Shell 调用。
    LocalShellCall(OpenAIResponsesLocalShellCallItem),
    /// 本地 Shell 调用输出。
    LocalShellCallOutput(OpenAIResponsesLocalShellCallOutputItem),
    /// Shell 调用。
    ShellCall(OpenAIResponsesShellCallItem),
    /// Shell 调用输出。
    ShellCallOutput(OpenAIResponsesShellCallOutputItem),
    /// Apply patch 调用。
    ApplyPatchCall(OpenAIResponsesApplyPatchCallItem),
    /// Apply patch 调用输出。
    ApplyPatchCallOutput(OpenAIResponsesApplyPatchCallOutputItem),
    /// MCP 工具列表。
    McpListTools(OpenAIResponsesMcpListToolsItem),
    /// MCP 审批请求。
    McpApprovalRequest(OpenAIResponsesMcpApprovalRequestItem),
    /// MCP 审批响应。
    McpApprovalResponse(OpenAIResponsesMcpApprovalResponseItem),
    /// MCP 调用。
    McpCall(OpenAIResponsesMcpCallItem),
    /// 自定义工具调用。
    CustomToolCall(OpenAIResponsesCustomToolCallItem),
    /// 自定义工具调用输出。
    CustomToolCallOutput(OpenAIResponsesCustomToolCallOutputItem),
    /// Item 引用。
    ItemReference(OpenAIResponsesItemReference),
    /// Compaction 项。
    Compaction(OpenAIResponsesCompactionItem),
    /// 未识别的 Item 类型——丢 `Unknown` 保留原 JSON。
    Unknown(serde_json::Value),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
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
    /// 客户端输入音频。
    InputAudio { input_audio: InputAudio },
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// 输出 text part（Responses API 响应里用）。
    OutputText {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<OpenAIResponsesAnnotation>,
    },
    /// assistant refusal part（作为历史 output message 回填到 input 时使用）。
    Refusal { refusal: String },
    /// 未识别 part 类型。
    #[serde(other)]
    Unknown,
}

/// input 里的 function_call item。
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// input 里的 function_call_output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFunctionCallOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    /// 工具执行结果。
    pub output: OpenAIResponsesFunctionCallOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// function_call_output 的输出可以是字符串或内容数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesFunctionCallOutput {
    Text(String),
    Parts(Vec<OpenAIResponsesInputContentPart>),
}

/// Reasoning item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesReasoningItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub summary: Vec<OpenAIResponsesReasoningSummaryContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<OpenAIResponsesReasoningTextContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesReasoningSummaryContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesReasoningTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: String,
}

/// File search call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFileSearchCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub queries: Vec<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<OpenAIResponsesFileSearchResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesFileSearchResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, Value>>,
}

/// Computer call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesComputerCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub pending_safety_checks: Vec<OpenAIResponsesSafetyCheck>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<OpenAIResponsesComputerAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<OpenAIResponsesComputerAction>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesSafetyCheck {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Computer action。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesComputerAction {
    Click {
        button: String,
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    DoubleClick {
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    Drag {
        path: Vec<OpenAIResponsesDragPoint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    Keypress {
        keys: Vec<String>,
    },
    Move {
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    Screenshot,
    Scroll {
        scroll_x: i64,
        scroll_y: i64,
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    Type {
        text: String,
    },
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesDragPoint {
    pub x: i64,
    pub y: i64,
}

/// Computer call output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesComputerCallOutputItem {
    pub call_id: String,
    pub output: OpenAIResponsesComputerScreenshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<OpenAIResponsesSafetyCheck>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesComputerScreenshot {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

/// Web search call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesWebSearchCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<OpenAIResponsesWebSearchAction>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesWebSearchAction {
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<OpenAIResponsesWebSearchSource>>,
    },
    OpenPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    FindInPage {
        pattern: String,
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesWebSearchSource {
    #[serde(rename = "type")]
    pub type_: String,
    pub url: String,
}

/// Tool search call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolSearchCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Tool search output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolSearchOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub tools: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Image generation call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesImageGenerationCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub status: String,
}

/// Code interpreter call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesCodeInterpreterCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<OpenAIResponsesCodeInterpreterOutput>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesCodeInterpreterOutput {
    Logs { logs: String },
    Image { url: String },
}

/// Local shell call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesLocalShellCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub action: OpenAIResponsesLocalShellAction,
    pub call_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesLocalShellAction {
    pub command: Vec<String>,
    pub env: HashMap<String, String>,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

/// Local shell call output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesLocalShellCallOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Shell call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesShellCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub action: OpenAIResponsesShellCallAction,
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesShellCallAction {
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
}

/// Shell call output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesShellCallOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: Vec<OpenAIResponsesShellCallOutputContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesShellCallOutputContent {
    pub stdout: String,
    pub stderr: String,
    pub outcome: OpenAIResponsesShellCallOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesShellCallOutcome {
    Timeout,
    Exit { exit_code: i64 },
}

/// Apply patch call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesApplyPatchCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub operation: OpenAIResponsesApplyPatchOperation,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesApplyPatchOperation {
    CreateFile { path: String, diff: String },
    DeleteFile { path: String },
    UpdateFile { path: String, diff: String },
}

/// Apply patch call output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesApplyPatchCallOutputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// MCP list tools item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMcpListToolsItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub server_label: String,
    pub tools: Vec<OpenAIResponsesMcpToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMcpToolDefinition {
    pub name: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

/// MCP approval request item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMcpApprovalRequestItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub arguments: String,
    pub name: String,
    pub server_label: String,
}

/// MCP approval response item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMcpApprovalResponseItem {
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// MCP call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesMcpCallItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub arguments: String,
    pub name: String,
    pub server_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Custom tool call item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesCustomToolCallItem {
    pub call_id: String,
    pub input: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// Custom tool call output item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesCustomToolCallOutputItem {
    pub call_id: String,
    pub output: OpenAIResponsesCustomToolCallOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesCustomToolCallOutput {
    Text(String),
    Parts(Vec<OpenAIResponsesInputContentPart>),
}

/// Item reference。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesItemReference {
    pub id: String,
}

/// Compaction item。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesCompactionItem {
    pub encrypted_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

// =========================================================================
// Tool Choice
// =========================================================================

/// 工具选择策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesToolChoice {
    /// 简单字符串选项。
    Simple(String),
    /// 指定函数调用。
    Function(OpenAIResponsesToolChoiceFunction),
    /// 指定 MCP 工具。
    Mcp(OpenAIResponsesToolChoiceMcp),
    /// 指定自定义工具。
    Custom(OpenAIResponsesToolChoiceCustom),
    /// 允许的工具列表。
    AllowedTools(OpenAIResponsesToolChoiceAllowed),
    /// 内置工具类型。
    Builtin(OpenAIResponsesToolChoiceTypes),
    /// Apply patch 工具。
    ApplyPatch(OpenAIResponsesToolChoiceApplyPatch),
    /// Shell 工具。
    Shell(OpenAIResponsesToolChoiceShell),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceFunction {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceMcp {
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceCustom {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceAllowed {
    pub mode: String,
    pub tools: Vec<Value>,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceTypes {
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceApplyPatch {
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesToolChoiceShell {
    #[serde(rename = "type")]
    pub type_: String,
}

// =========================================================================
// Tools
// =========================================================================

/// 请求 `tools` 字段里的 tool。
#[derive(Debug, Clone, PartialEq)]
pub enum OpenAIResponsesTool {
    /// 普通 function tool。
    Function(OpenAIResponsesFunctionTool),
    /// OpenAI built-in / provider-native tool。
    Builtin {
        kind: String,
        extra: serde_json::Map<String, Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAIResponsesFunctionTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
}

impl Serialize for OpenAIResponsesTool {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match self {
            Self::Function(tool) => {
                map.serialize_entry("type", "function")?;
                map.serialize_entry("name", &tool.name)?;
                if let Some(description) = &tool.description {
                    map.serialize_entry("description", description)?;
                }
                if let Some(parameters) = &tool.parameters {
                    map.serialize_entry("parameters", parameters)?;
                }
                if let Some(strict) = tool.strict {
                    map.serialize_entry("strict", &strict)?;
                }
                if let Some(defer_loading) = tool.defer_loading {
                    map.serialize_entry("defer_loading", &defer_loading)?;
                }
            }
            Self::Builtin { kind, extra } => {
                map.serialize_entry("type", kind)?;
                for (k, v) in extra {
                    map.serialize_entry(k, v)?;
                }
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for OpenAIResponsesTool {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let mut obj = match value {
            Value::Object(obj) => obj,
            _ => return Err(de::Error::custom("responses tool must be an object")),
        };
        let kind = obj
            .remove("type")
            .and_then(|v| v.as_str().map(ToOwned::to_owned))
            .ok_or_else(|| de::Error::custom("responses tool missing string field `type`"))?;

        if kind == "function" {
            let tool: OpenAIResponsesFunctionTool =
                serde_json::from_value(Value::Object(obj)).map_err(de::Error::custom)?;
            Ok(Self::Function(tool))
        } else {
            Ok(Self::Builtin { kind, extra: obj })
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub model: String,
    /// `completed` / `in_progress` / `failed` / `incomplete` / `cancelled`。
    pub status: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<OpenAIResponsesOutputItem>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIResponsesUsage>,

    /// 所有 output message 里 text parts 的拼接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<OpenAIResponsesIncompleteDetails>,

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
    pub error: Option<OpenAIResponsesError>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesIncompleteDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 响应 `output` 数组里的 Item。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesOutputItem {
    Message(OpenAIResponsesOutputMessage),
    FunctionCall(OpenAIResponsesOutputFunctionCall),
    Reasoning(OpenAIResponsesReasoningItem),
    FileSearchCall(OpenAIResponsesFileSearchCallItem),
    ComputerCall(OpenAIResponsesComputerCallItem),
    ComputerCallOutput(OpenAIResponsesComputerCallOutputItem),
    WebSearchCall(OpenAIResponsesWebSearchCallItem),
    ToolSearchCall(OpenAIResponsesToolSearchCallItem),
    ToolSearchOutput(OpenAIResponsesToolSearchOutputItem),
    ImageGenerationCall(OpenAIResponsesImageGenerationCallItem),
    CodeInterpreterCall(OpenAIResponsesCodeInterpreterCallItem),
    LocalShellCall(OpenAIResponsesLocalShellCallItem),
    LocalShellCallOutput(OpenAIResponsesLocalShellCallOutputItem),
    ShellCall(OpenAIResponsesShellCallItem),
    ShellCallOutput(OpenAIResponsesShellCallOutputItem),
    ApplyPatchCall(OpenAIResponsesApplyPatchCallItem),
    ApplyPatchCallOutput(OpenAIResponsesApplyPatchCallOutputItem),
    McpListTools(OpenAIResponsesMcpListToolsItem),
    McpApprovalRequest(OpenAIResponsesMcpApprovalRequestItem),
    McpApprovalResponse(OpenAIResponsesMcpApprovalResponseItem),
    McpCall(OpenAIResponsesMcpCallItem),
    CustomToolCall(OpenAIResponsesCustomToolCallItem),
    CustomToolCallOutput(OpenAIResponsesCustomToolCallOutputItem),
    ItemReference(OpenAIResponsesItemReference),
    Compaction(OpenAIResponsesCompactionItem),
    /// 未识别 item 类型。
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesOutputContentPart {
    OutputText {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<OpenAIResponsesAnnotation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<OpenAIResponsesLogprob>>,
    },
    Refusal {
        refusal: String,
    },
    /// 未识别 part 类型。
    #[serde(other)]
    Unknown,
}

/// 注解类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIResponsesAnnotation {
    FileCitation {
        file_id: String,
        filename: String,
        index: i64,
    },
    UrlCitation {
        end_index: i64,
        start_index: i64,
        title: String,
        url: String,
    },
    ContainerFileCitation {
        container_id: String,
        end_index: i64,
        file_id: String,
        filename: String,
        start_index: i64,
    },
    FilePath {
        file_id: String,
        index: i64,
    },
}

/// Logprob 信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesLogprob {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<i64>>,
    pub logprob: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<OpenAIResponsesTopLogprob>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesTopLogprob {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<i64>>,
    pub logprob: f64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// Responses API 自家 usage 结构。
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
// Filters (for FileSearch)
// =========================================================================

/// 比较过滤器。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesComparisonFilter {
    pub key: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub value: OpenAIResponsesFilterValue,
}

/// 过滤器值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesFilterValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<OpenAIResponsesFilterArrayValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIResponsesFilterArrayValue {
    String(String),
    Number(f64),
}

/// 复合过滤器。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponsesCompoundFilter {
    pub filters: Vec<Value>,
    #[serde(rename = "type")]
    pub type_: String,
}

// =========================================================================
// Stream events
// =========================================================================

/// Responses API 流事件。
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
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        summary_index: u32,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone {
        item_id: String,
        output_index: u32,
        summary_index: u32,
        text: String,
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
// Helpers
// =========================================================================

/// 从 canonical `Tool` 转成 Responses API tool。
impl From<Tool> for OpenAIResponsesTool {
    fn from(t: Tool) -> Self {
        if t.is_function() {
            let func = t.function.unwrap_or(crate::types::common::ToolFunction {
                name: String::new(),
                description: None,
                parameters: None,
            });
            Self::Function(OpenAIResponsesFunctionTool {
                name: func.name,
                description: func.description,
                parameters: func.parameters,
                strict: t.strict,
                defer_loading: None,
            })
        } else {
            Self::Builtin {
                kind: t.kind,
                extra: t.extra,
            }
        }
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
// Tests
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
            {"type":"reasoning","id":"r1","summary":[{"text":"test","type":"summary_text"}]}
        ]))
        .unwrap();
        // Now reasoning is a known variant, so it should parse as Reasoning
        matches!(items[0], OpenAIResponsesInputItem::Reasoning(_));
    }

    #[test]
    fn tool_choice_simple_parses() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hello",
            "tool_choice": "auto"
        }))
        .unwrap();
        if let Some(OpenAIResponsesToolChoice::Simple(s)) = &req.tool_choice {
            assert_eq!(s, "auto");
        } else {
            panic!("expected Simple tool_choice");
        }
    }

    #[test]
    fn tool_choice_function_parses() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hello",
            "tool_choice": {"type": "function", "name": "get_weather"}
        }))
        .unwrap();
        if let Some(OpenAIResponsesToolChoice::Function(f)) = &req.tool_choice {
            assert_eq!(f.name, "get_weather");
        } else {
            panic!("expected Function tool_choice");
        }
    }

    #[test]
    fn response_roundtrips_minimal_message_output() {
        let resp = OpenAIResponsesResponse {
            id: "resp_1".into(),
            object: "response".into(),
            created_at: 0,
            completed_at: Some(1),
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
                        logprobs: None,
                    }],
                    phase: None,
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
            service_tier: None,
            store: None,
            text: None,
            truncation: None,
            top_logprobs: None,
            prompt: None,
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

    #[test]
    fn reasoning_effort_parses() {
        let reasoning: OpenAIResponsesReasoning = serde_json::from_value(serde_json::json!({
            "effort": "high",
            "summary": "concise"
        }))
        .unwrap();
        assert!(matches!(
            reasoning.effort,
            Some(OpenAIResponsesReasoningEffort::High)
        ));
        assert!(matches!(
            reasoning.summary,
            Some(OpenAIResponsesReasoningSummary::Concise)
        ));
    }

    #[test]
    fn text_format_json_schema_parses() {
        let config: OpenAIResponsesTextConfig = serde_json::from_value(serde_json::json!({
            "format": {
                "type": "json_schema",
                "name": "my_schema",
                "schema": {"type": "object"}
            }
        }))
        .unwrap();
        if let Some(OpenAIResponsesTextFormat::JsonSchema { name, .. }) = config.format {
            assert_eq!(name, "my_schema");
        } else {
            panic!("expected JsonSchema format");
        }
    }
}
