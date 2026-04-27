//! Claude Messages API 的 wire 类型定义。
//!
//! 严格对齐 [Claude Messages API](https://docs.anthropic.com/en/api/messages)
//! 和 [Streaming](https://docs.anthropic.com/en/api/messages-streaming)。
//!
//! # 设计原则
//!
//! - **纯 struct + serde**，无转换逻辑——converter 在 `relay/src/convert/` 实现
//! - 字段用 `Option<T>` + `skip_serializing_if = "Option::is_none"` 保证
//!   "缺省"和"null"语义可区分
//! - `Vec<T>` 用 `skip_serializing_if = "Vec::is_empty"`，空数组不发送
//! - 枚举用 `#[serde(tag = "type", rename_all = "snake_case")]` 匹配 Claude
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
pub struct ClaudeMessagesRequest {
    pub model: String,
    pub messages: Vec<ClaudeMessage>,
    /// Claude 必填字段（不像 OpenAI 可选）。
    pub max_tokens: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<ClaudeSystem>,

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
    pub tools: Vec<ClaudeTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ClaudeToolChoice>,

    /// Extended thinking（claude-3.7 / claude-4 系列支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ClaudeMetadata>,

    /// 透传私有 / 未覆盖字段。
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `system` 字段可以是字符串或多块（均可带 `cache_control`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaudeSystem {
    Text(String),
    Blocks(Vec<ClaudeSystemBlock>),
}

/// `system` 数组形态的元素（只含 text + cache_control）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSystemBlock {
    #[serde(rename = "type")]
    pub kind: String, // 固定 "text"
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Claude 消息（user / assistant）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessage {
    /// `"user"` | `"assistant"`
    pub role: String,
    pub content: ClaudeContent,
}

/// 消息 content：字符串 or 多块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaudeContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

/// Claude content block 的全部类型。
///
/// 支持所有官方类型，包括：
/// - Text: 文本块（可选 citations）
/// - Image: 图像块
/// - ToolUse: 工具调用（可选 caller）
/// - ToolResult: 工具结果
/// - Thinking: 扩展思考
/// - RedactedThinking: 被过滤的思考
/// - Document: PDF/文档输入
/// - SearchResult: 搜索结果块（新增）
/// - ServerToolUse: 服务器工具调用（新增）
/// - WebSearchToolResult: 网络搜索工具结果（新增）
/// - CodeExecutionResult: 代码执行结果（新增）
/// - ContainerUpload: 容器文件上传（新增）
/// - Unknown: 未识别的类型（兜底）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeContentBlock {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<TextCitation>>,
    },
    Image {
        source: ClaudeImageSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<ClaudeToolResultContent>,
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
    /// 被 Claude 过滤的 thinking（只剩加密 data）。
    RedactedThinking { data: String },
    /// PDF / 文档输入。source 结构多样，先用 Value 兜底。
    Document {
        source: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        citations: Option<DocumentCitationsConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// 搜索结果块（新增）。
    SearchResult {
        content: Vec<ClaudeContentBlock>,
        source: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<TextCitation>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    /// 服务器工具调用（新增）。
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// 网络搜索工具结果（新增）。
    WebSearchToolResult {
        tool_use_id: String,
        content: WebSearchToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// 网络抓取工具结果（新增）。
    WebFetchToolResult {
        tool_use_id: String,
        content: WebFetchToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// 代码执行工具结果（新增）。
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult {
        tool_use_id: String,
        content: CodeExecutionToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// Bash 代码执行工具结果（新增）。
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult {
        tool_use_id: String,
        content: BashCodeExecutionToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 文本编辑器代码执行工具结果（新增）。
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult {
        tool_use_id: String,
        content: TextEditorCodeExecutionToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 工具搜索工具结果（新增）。
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult {
        tool_use_id: String,
        content: ToolSearchToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 容器文件上传（新增）。
    ContainerUpload {
        file_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 工具引用（通常嵌在 tool_search_tool_result.content.tool_references 里）。
    ToolReference {
        tool_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 未识别的 block type（反序列化兜底）。
    ///
    /// `#[serde(other)]` 只能是 unit variant，原始 JSON 在此丢失——
    /// 这是 serde 的限制。若未来需要完整透传，应改成自定义 `Deserialize` 把
    /// raw JSON 存进一个 `Value` 字段。
    #[serde(other)]
    Unknown,
}

/// 图像 source：base64 或 URL。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// 文本引用（用于 citations）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextCitation {
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        start_char_index: u32,
        end_char_index: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
    },
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        start_page_number: u32,
        end_page_number: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
    },
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        start_block_index: u32,
        end_block_index: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
    },
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        title: String,
        url: String,
    },
    SearchResultLocation {
        cited_text: String,
        end_block_index: u32,
        search_result_index: u32,
        source: String,
        start_block_index: u32,
        title: String,
    },
}

/// 工具调用者（用于 ToolUse.caller）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCaller {
    Direct,
    CodeExecution20250825 { tool_id: String },
    CodeExecution20260120 { tool_id: String },
}

/// 网络搜索工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSearchToolResultContent {
    WebSearchResult(Vec<WebSearchResultItem>),
    Error(WebSearchToolResultError),
}

/// 文档引用配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCitationsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// 网络搜索工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolResultError {
    pub error_code: WebSearchToolErrorCode,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 网络搜索结果项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResultItem {
    pub encrypted_content: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String, // "web_search_result"
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

/// 网络搜索工具错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchToolErrorCode {
    InvalidToolInput,
    Unavailable,
    MaxUsesExceeded,
    TooManyRequests,
    QueryTooLong,
    RequestTooLarge,
}

/// 网络抓取工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebFetchToolResultContent {
    Result(WebFetchResultBlock),
    Error(WebFetchToolResultError),
}

/// 网络抓取工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchToolResultError {
    pub error_code: WebFetchToolErrorCode,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 网络抓取结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchResultBlock {
    pub content: serde_json::Value,
    pub retrieved_at: String,
    #[serde(rename = "type")]
    pub kind: String, // "web_fetch_result"
    pub url: String,
}

/// 网络抓取工具错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebFetchToolErrorCode {
    InvalidToolInput,
    UrlTooLong,
    UrlNotAllowed,
    UrlNotAccessible,
    UnsupportedContentType,
    TooManyRequests,
    MaxUsesExceeded,
    Unavailable,
}

/// 代码执行工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeExecutionToolResultContent {
    Result(CodeExecutionResultBlock),
    EncryptedResult(EncryptedCodeExecutionResultBlock),
    Error(CodeExecutionToolResultError),
}

/// 代码执行工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionToolResultError {
    pub error_code: CodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 代码执行结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionResultBlock {
    pub content: Vec<CodeExecutionOutput>,
    pub return_code: i32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub kind: String, // "code_execution_result"
}

/// 加密代码执行结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedCodeExecutionResultBlock {
    pub content: Vec<CodeExecutionOutput>,
    pub encrypted_stdout: String,
    pub return_code: i32,
    pub stderr: String,
    #[serde(rename = "type")]
    pub kind: String, // "encrypted_code_execution_result"
}

/// 代码执行工具结果错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}

/// tool_result 的 content：字符串或多块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaudeToolResultContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

/// Prompt cache 控制（Claude 独有）。
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
///
/// Claude `/v1/messages` 的 `tools` 字段兼容两种形态：
///
/// 1. **Custom function tool** ——  `{name, description, input_schema, cache_control}`。
///    不带 `type` 字段；`input_schema` 必填；adapter 映射 canonical `Tool::function`。
///
/// 2. **Server tool / built-in** —— `{type, name, ...config}`。比如：
///    - `web_search_20250305`：带 `max_uses` / `allowed_domains` / `blocked_domains`
///      / `user_location`
///    - `computer_20241022` / `text_editor_20250728` / `bash_20241022`
///    - `mcp_connector_20250716`：带 `server_url` / `server_label` /
///      `authorization_token` / `allowed_tools`
///
/// wire 上区别：built-in 有 `type`，custom 没有。`input_schema` custom 必填，built-in
/// 不允许。`#[serde(flatten)] extra` 承载任意 built-in 配置字段，保证 adapter 翻译时
/// 不丢字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTool {
    /// Built-in tool 的 `type` 字段（`web_search_20250305` / `mcp_connector_20250716`
    /// 等）；custom function tool 不带。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Custom function tool 的 JSON Schema；built-in 不允许带此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// Built-in 工具的私有配置（`max_uses` / `allowed_domains` / `server_url` 等）。
    /// custom tool 留空；built-in 平铺承载，wire 上与 `name` 同层。
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 代码执行输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionOutput {
    pub file_id: String,
    #[serde(rename = "type")]
    pub kind: String, // "code_execution_output"
}

/// Bash 代码执行工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BashCodeExecutionToolResultContent {
    Result(BashCodeExecutionResultBlock),
    Error(BashCodeExecutionToolResultError),
}

/// Bash 代码执行结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCodeExecutionResultBlock {
    pub content: Vec<BashCodeExecutionOutput>,
    pub return_code: i32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub kind: String, // "bash_code_execution_result"
}

/// Bash 代码执行工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCodeExecutionToolResultError {
    pub error_code: BashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub kind: String,
}

/// Bash 代码执行输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCodeExecutionOutput {
    pub file_id: String,
    #[serde(rename = "type")]
    pub kind: String, // "bash_code_execution_output"
}

/// Bash 代码执行工具结果错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BashCodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    OutputFileTooLarge,
}

/// 文本编辑器代码执行工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextEditorCodeExecutionToolResultContent {
    ViewResult(TextEditorCodeExecutionViewResultBlock),
    CreateResult(TextEditorCodeExecutionCreateResultBlock),
    StrReplaceResult(TextEditorCodeExecutionStrReplaceResultBlock),
    Error(TextEditorCodeExecutionToolResultError),
}

/// 文本编辑器代码执行工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionToolResultError {
    pub error_code: TextEditorCodeExecutionToolResultErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 文本编辑器 view 结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u32>,
    #[serde(rename = "type")]
    pub kind: String, // "text_editor_code_execution_view_result"
}

/// 文本编辑器 create 结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionCreateResultBlock {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub kind: String, // "text_editor_code_execution_create_result"
}

/// 文本编辑器 str_replace 结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionStrReplaceResultBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_start: Option<u32>,
    #[serde(rename = "type")]
    pub kind: String, // "text_editor_code_execution_str_replace_result"
}

/// 文本编辑器代码执行工具错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEditorCodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    FileNotFound,
}

/// 工具搜索工具结果内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSearchToolResultContent {
    SearchResult(ToolSearchToolSearchResultBlock),
    Error(ToolSearchToolResultError),
}

/// 工具搜索工具结果错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchToolResultError {
    pub error_code: ToolSearchToolResultErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 工具搜索结果块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchToolSearchResultBlock {
    pub tool_references: Vec<ToolReferenceBlock>,
    #[serde(rename = "type")]
    pub kind: String, // "tool_search_tool_search_result"
}

/// 工具引用块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReferenceBlock {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub kind: String, // "tool_reference"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// 工具搜索工具错误码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}

/// `tool_choice` 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeToolChoice {
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

/// 用户侧元数据（Claude abuse 检测用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Response (non-stream)
// ---------------------------------------------------------------------------

/// 非流式响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeResponse {
    pub id: String,
    #[serde(rename = "type", default = "default_message_type")]
    pub kind: String, // "message"
    pub role: String, // "assistant"
    pub content: Vec<ClaudeContentBlock>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<ClaudeStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: ClaudeUsage,
}

/// Claude 停止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeStopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    Refusal,
    PauseTurn,
}

/// Claude usage（含 prompt cache 计费字段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeUsage {
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
    pub cache_creation: Option<ClaudeCacheCreation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

/// Cache creation 5m/1h 细分。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCacheCreation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral_5m_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral_1h_input_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Stream events (6 种 + ping + error)
// ---------------------------------------------------------------------------

/// Claude SSE 事件。
///
/// 正常序列：`message_start` → (`content_block_start` → `content_block_delta*`
/// → `content_block_stop`)+ → `message_delta` → `message_stop`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeStreamEvent {
    MessageStart {
        message: ClaudeStreamMessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: ClaudeStreamContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: ClaudeStreamDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: ClaudeStreamMessageDelta,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<ClaudeUsage>,
    },
    MessageStop,
    /// 保活。客户端忽略即可。
    Ping,
    /// 流中错误。
    Error {
        error: ClaudeErrorBody,
    },
}

/// `message_start` 事件里的 message 对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStreamMessageStart {
    pub id: String,
    #[serde(rename = "type", default = "default_message_type")]
    pub kind: String,
    pub role: String,
    #[serde(default)]
    pub content: Vec<ClaudeContentBlock>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<ClaudeStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: ClaudeUsage,
}

/// `content_block_start` 里 `content_block` 字段的 block 类型（不含 cache_control，
/// 流里不会再带 cache hint）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeStreamContentBlock {
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
pub enum ClaudeStreamDelta {
    TextDelta {
        text: String,
    },
    /// tool_use 的 `arguments` 是 JSON 字符串增量——**Claude 官方就是 string**，
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
pub struct ClaudeStreamMessageDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<ClaudeStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// `error` 事件的 body。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeErrorBody {
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
    use serde_json::json;

    #[test]
    fn minimal_request_roundtrip() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.max_tokens, 64);
        assert_eq!(req.messages.len(), 1);
        assert!(matches!(req.messages[0].content, ClaudeContent::Text(_)));
        assert!(!req.stream);
    }

    #[test]
    fn system_can_be_string_or_array() {
        let s: ClaudeSystem = serde_json::from_value(serde_json::json!("you are helpful")).unwrap();
        assert!(matches!(s, ClaudeSystem::Text(_)));

        let b: ClaudeSystem = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "A"},
            {"type": "text", "text": "B", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
        ]))
        .unwrap();
        match b {
            ClaudeSystem::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert!(blocks[1].cache_control.is_some());
            }
            _ => panic!("expected Blocks"),
        }
    }

    #[test]
    fn content_blocks_tool_use_and_result() {
        let blocks: Vec<ClaudeContentBlock> = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "let me check"},
            {"type": "tool_use", "id": "tu_1", "name": "weather", "input": {"city": "NYC"}}
        ]))
        .unwrap();
        assert!(matches!(blocks[0], ClaudeContentBlock::Text { .. }));
        assert!(matches!(blocks[1], ClaudeContentBlock::ToolUse { .. }));

        let result: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "tu_1",
            "content": "72F"
        }))
        .unwrap();
        match result {
            ClaudeContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "tu_1");
                assert!(matches!(content, Some(ClaudeToolResultContent::Text(_))));
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn tool_choice_variants() {
        let auto: ClaudeToolChoice =
            serde_json::from_value(serde_json::json!({"type": "auto"})).unwrap();
        assert!(matches!(auto, ClaudeToolChoice::Auto { .. }));

        let tool: ClaudeToolChoice =
            serde_json::from_value(serde_json::json!({"type": "tool", "name": "weather"})).unwrap();
        match tool {
            ClaudeToolChoice::Tool { name, .. } => assert_eq!(name, "weather"),
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
        let e: ClaudeStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            ClaudeStreamEvent::MessageStart { message } => {
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
        let e: ClaudeStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            ClaudeStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    ClaudeStreamDelta::TextDelta { text } => assert_eq!(text, "hello"),
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
        let e: ClaudeStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            ClaudeStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                ClaudeStreamDelta::InputJsonDelta { partial_json } => {
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
        let e: ClaudeStreamEvent = serde_json::from_str(raw).unwrap();
        match e {
            ClaudeStreamEvent::MessageDelta { delta, usage } => {
                assert_eq!(delta.stop_reason, Some(ClaudeStopReason::EndTurn));
                assert_eq!(usage.unwrap().output_tokens, 12);
            }
            _ => panic!("expected MessageDelta"),
        }
    }

    #[test]
    fn stream_event_message_stop_and_ping() {
        let stop: ClaudeStreamEvent = serde_json::from_str(r#"{"type":"message_stop"}"#).unwrap();
        assert!(matches!(stop, ClaudeStreamEvent::MessageStop));

        let ping: ClaudeStreamEvent = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(ping, ClaudeStreamEvent::Ping));
    }

    #[test]
    fn usage_roundtrips_cache_fields() {
        let u: ClaudeUsage = serde_json::from_value(serde_json::json!({
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

    #[test]
    fn new_content_block_types_deserialize_to_concrete_variants() {
        // 这些新增 block 已经进入本地 enum：反序列化必须拿到具体 variant，
        // 后续 adapter / ingress 才能按业务语义处理，而不是退回 Unknown。
        let b: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "server_tool_use",
            "id": "srvtu_1",
            "name": "web_search",
            "input": {"query": "weather"}
        }))
        .unwrap();
        assert!(matches!(b, ClaudeContentBlock::ServerToolUse { .. }));

        // 嵌在消息数组里也能正常跟其他 block 共存
        let blocks: Vec<ClaudeContentBlock> = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "hi"},
            {"type": "container_upload", "file_id": "f_1"},
            {"type": "text", "text": "bye"}
        ]))
        .unwrap();
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[0], ClaudeContentBlock::Text { .. }));
        assert!(matches!(
            blocks[1],
            ClaudeContentBlock::ContainerUpload { .. }
        ));
        assert!(matches!(blocks[2], ClaudeContentBlock::Text { .. }));
    }

    #[test]
    fn docs_defined_tool_result_blocks_do_not_fall_back_to_unknown() {
        let code_exec: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "code_execution_tool_result",
            "tool_use_id": "srvtu_code",
            "content": {
                "type": "code_execution_result",
                "content": [
                    {"type": "code_execution_output", "file_id": "file_1"}
                ],
                "stdout": "ok",
                "stderr": "",
                "return_code": 0
            }
        }))
        .unwrap();
        assert!(matches!(
            code_exec,
            ClaudeContentBlock::CodeExecutionToolResult { .. }
        ));
        let code_exec_json = serde_json::to_value(&code_exec).unwrap();
        assert_eq!(code_exec_json["type"], "code_execution_tool_result");
        assert_eq!(code_exec_json["content"]["type"], "code_execution_result");

        let bash_exec: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "bash_code_execution_tool_result",
            "tool_use_id": "srvtu_bash",
            "content": {
                "type": "bash_code_execution_result",
                "content": [
                    {"type": "bash_code_execution_output", "file_id": "file_2"}
                ],
                "stdout": "ls",
                "stderr": "",
                "return_code": 0
            }
        }))
        .unwrap();
        assert!(matches!(
            bash_exec,
            ClaudeContentBlock::BashCodeExecutionToolResult { .. }
        ));
        let bash_exec_json = serde_json::to_value(&bash_exec).unwrap();
        assert_eq!(bash_exec_json["type"], "bash_code_execution_tool_result");
        assert_eq!(
            bash_exec_json["content"]["type"],
            "bash_code_execution_result"
        );

        let text_editor: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "text_editor_code_execution_tool_result",
            "tool_use_id": "srvtu_edit",
            "content": {
                "type": "text_editor_code_execution_view_result",
                "content": "fn main() {}",
                "file_type": "text",
                "start_line": 1,
                "num_lines": 1,
                "total_lines": 1
            }
        }))
        .unwrap();
        assert!(matches!(
            text_editor,
            ClaudeContentBlock::TextEditorCodeExecutionToolResult { .. }
        ));
        let text_editor_json = serde_json::to_value(&text_editor).unwrap();
        assert_eq!(
            text_editor_json["type"],
            "text_editor_code_execution_tool_result"
        );
        assert_eq!(
            text_editor_json["content"]["type"],
            "text_editor_code_execution_view_result"
        );

        let tool_search: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "tool_search_tool_result",
            "tool_use_id": "srvtu_search",
            "content": {
                "type": "tool_search_tool_search_result",
                "tool_references": [
                    {"type": "tool_reference", "tool_name": "web_search"}
                ]
            }
        }))
        .unwrap();
        assert!(matches!(
            tool_search,
            ClaudeContentBlock::ToolSearchToolResult { .. }
        ));
        let tool_search_json = serde_json::to_value(&tool_search).unwrap();
        assert_eq!(tool_search_json["type"], "tool_search_tool_result");
        assert_eq!(
            tool_search_json["content"]["type"],
            "tool_search_tool_search_result"
        );

        let web_fetch: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "web_fetch_tool_result",
            "tool_use_id": "srvtu_fetch",
            "content": {
                "type": "web_fetch_result",
                "url": "https://example.com/doc",
                "retrieved_at": "2026-04-26T12:00:00Z",
                "content": {
                    "type": "document",
                    "title": "Example",
                    "source": {
                        "type": "text",
                        "media_type": "text/plain",
                        "data": "hello"
                    }
                }
            }
        }))
        .unwrap();
        assert!(matches!(
            web_fetch,
            ClaudeContentBlock::WebFetchToolResult { .. }
        ));
        let web_fetch_json = serde_json::to_value(&web_fetch).unwrap();
        assert_eq!(web_fetch_json["type"], "web_fetch_tool_result");
        assert_eq!(web_fetch_json["content"]["type"], "web_fetch_result");
    }

    #[test]
    fn document_and_text_citation_fields_roundtrip_from_docs() {
        let document: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "document",
            "title": "Example Doc",
            "context": "snippet",
            "citations": {"enabled": true},
            "source": {
                "type": "text",
                "media_type": "text/plain",
                "data": "hello"
            }
        }))
        .unwrap();
        let document_json = serde_json::to_value(&document).unwrap();
        assert_eq!(document_json["title"], "Example Doc");
        assert_eq!(document_json["context"], "snippet");
        assert_eq!(document_json["citations"]["enabled"], true);

        let text: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "text",
            "text": "answer",
            "citations": [{
                "type": "char_location",
                "cited_text": "answer",
                "document_index": 0,
                "document_title": "Example Doc",
                "start_char_index": 0,
                "end_char_index": 6,
                "file_id": "file_123"
            }]
        }))
        .unwrap();
        let text_json = serde_json::to_value(&text).unwrap();
        assert_eq!(text_json["citations"][0]["file_id"], "file_123");
    }

    #[test]
    fn request_side_optional_tool_result_fields_from_docs_deserialize() {
        let text_editor: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "text_editor_code_execution_tool_result",
            "tool_use_id": "srvtu_edit",
            "cache_control": {"type": "ephemeral"},
            "content": {
                "type": "text_editor_code_execution_view_result",
                "content": "fn main() {}",
                "file_type": "text"
            }
        }))
        .unwrap();
        let text_editor_json = serde_json::to_value(&text_editor).unwrap();
        assert_eq!(text_editor_json["cache_control"]["type"], "ephemeral");
        assert!(text_editor_json["content"].get("num_lines").is_none());

        let tool_search: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "tool_search_tool_result",
            "tool_use_id": "srvtu_search",
            "cache_control": {"type": "ephemeral"},
            "content": {
                "type": "tool_search_tool_search_result",
                "tool_references": [{
                    "type": "tool_reference",
                    "tool_name": "web_search",
                    "cache_control": {"type": "ephemeral", "ttl": "1h"}
                }]
            }
        }))
        .unwrap();
        let tool_search_json = serde_json::to_value(&tool_search).unwrap();
        assert_eq!(tool_search_json["cache_control"]["type"], "ephemeral");
        assert_eq!(
            tool_search_json["content"]["tool_references"][0]["cache_control"]["ttl"],
            "1h"
        );

        let tool_search_error: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "tool_search_tool_result",
            "tool_use_id": "srvtu_search",
            "content": {
                "type": "tool_search_tool_result_error",
                "error_code": "unavailable"
            }
        }))
        .unwrap();
        let tool_search_error_json = serde_json::to_value(&tool_search_error).unwrap();
        assert_eq!(
            tool_search_error_json["content"]["type"],
            "tool_search_tool_result_error"
        );

        let web_search: ClaudeContentBlock = serde_json::from_value(serde_json::json!({
            "type": "web_search_tool_result",
            "tool_use_id": "srvtu_web",
            "cache_control": {"type": "ephemeral"},
            "caller": {"type": "direct"},
            "content": [{
                "type": "web_search_result",
                "encrypted_content": "enc",
                "title": "Weather",
                "url": "https://example.com"
            }]
        }))
        .unwrap();
        let web_search_json = serde_json::to_value(&web_search).unwrap();
        assert_eq!(web_search_json["cache_control"]["type"], "ephemeral");
        assert_eq!(web_search_json["caller"]["type"], "direct");
    }
    #[tokio::test]
    async fn send_claude_with_tools_and_thinking() -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        // 构造一个带工具和 thinking 的请求
        let request = ClaudeMessagesRequest {
            model: "GLM-5.1".to_string(),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Get the weather in Tokyo and Paris".to_string()),
            }],
            max_tokens: 2000,
            system: None,
            temperature: Some(0.5),
            top_p: None,
            top_k: None,
            stop_sequences: vec![],
            stream: false,
            tools: vec![ClaudeTool {
                kind: None, // custom tool 没有 type
                name: "get_weather".to_string(),
                description: Some("Get current weather for a location".to_string()),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "location": { "type": "string" }
                    },
                    "required": ["location"]
                })),
                cache_control: None,
                extra: serde_json::Map::new(),
            }],
            tool_choice: Some(ClaudeToolChoice::Auto {
                disable_parallel_tool_use: Some(false),
            }),
            thinking: Some(ThinkingConfig::Enabled {
                budget_tokens: 2048,
            }),
            metadata: Some(ClaudeMetadata {
                user_id: Some("test_user".to_string()),
            }),
            extra: serde_json::Map::new(),
        };

        let response = client
            .post("http://localhost:8080/api/v1/messages")
            .bearer_auth("sk-test")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        println!("Claude (with tools) Status: {}", status);
        println!("Response: {}", body);
        Ok(())
    }
}
