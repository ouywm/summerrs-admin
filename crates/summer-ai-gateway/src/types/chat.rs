use serde::{Deserialize, Serialize};

use super::common::{FinishReason, Message, Usage};

/// POST /v1/chat/completions 请求体
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    // TODO: tools, tool_choice, response_format, etc.
}

/// 非流式响应
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub index: i32,
    pub message: Message,
    pub finish_reason: Option<FinishReason>,
}

/// 流式 chunk（SSE data 行）
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkChoice {
    pub index: i32,
    pub delta: Delta,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Delta {
    pub role: Option<String>,
    pub content: Option<String>,
}
