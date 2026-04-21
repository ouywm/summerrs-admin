//! Google Gemini GenerateContent API 的 wire 类型定义。
//!
//! 对齐 [Gemini GenerateContent](https://ai.google.dev/api/generate-content)。
//!
//! # 设计要点
//!
//! - Gemini 所有 JSON 字段都用 camelCase（`systemInstruction` / `generationConfig` / …）
//! - `Part` 是**单字段 tagged enum**（`{"text": "..."}` / `{"functionCall": {...}}` 等），
//!   用 `#[serde(untagged)]` + 单字段 struct 变体表达
//! - Role 在 canonical 是 `assistant`，Gemini 是 `model`——转换在 converter 做，wire 保留原值

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// `POST /v1beta/models/{model}:generateContent` 请求体。
///
/// 注意 `model` 字段在 URL 里（不在 body）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerateContentRequest {
    pub contents: Vec<GeminiContent>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiSystemInstruction>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<GeminiTool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<GeminiToolConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GeminiGenerationConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_settings: Vec<GeminiSafetySetting>,

    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Gemini 对话内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    /// `"user"` | `"model"` | `"function"`（旧）/ 空（system via `systemInstruction`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub parts: Vec<GeminiPart>,
}

/// `systemInstruction` 字段（只含 parts，没 role）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSystemInstruction {
    pub parts: Vec<GeminiPart>,
}

/// Gemini Part —— 多种 part 形态，用 untagged enum 表达。
///
/// 字段组合变体（protocol nuances）：
/// - `Text { text, thought: false, .. }` — 正常文本输出。
/// - `Text { text, thought: true, .. }` — Gemini 2.5 legacy 的思考链文本；
///   直接当 reasoning 处理，不能混进普通 content。
/// - `Text { text, thought_signature: Some(_), .. }` — Gemini 3+ 在正文 part
///   上直接挂 signature。signature 要透传给客户端做 multi-turn 续接。
/// - `ThoughtSignature { thought_signature }` — Gemini 3+ 独立的 signature-only
///   part（无 text 字段）。
///
/// 变体顺序决定 untagged 反序列化优先级：`Text` 在前以便正常文本/thought legacy
/// 能被识别；`ThoughtSignature` 放后面，`{"thoughtSignature":"..."}` 缺少 text
/// 字段时 Text 变体失败 → 自动 fallback 到此变体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text {
        text: String,
        /// Gemini 2.5 legacy：`thought=true` 表示这段 text 是思考链，不是正文。
        #[serde(default, skip_serializing_if = "is_false")]
        thought: bool,
        /// Gemini 3+ 的 thought signature（multi-turn 续接凭证）。
        #[serde(
            rename = "thoughtSignature",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        thought_signature: Option<String>,
    },
    /// 独立的 signature-only part（无 text 字段；Gemini 3+）。
    ThoughtSignature {
        #[serde(rename = "thoughtSignature")]
        thought_signature: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    FileData {
        #[serde(rename = "fileData")]
        file_data: GeminiFileData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
    /// 代码执行 / 视频 metadata 等 —— 透传。
    Other(serde_json::Value),
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl GeminiPart {
    /// 纯文本 part，`thought=false` 且无 signature —— 最常见的构造路径。
    pub fn plain_text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            thought: false,
            thought_signature: None,
        }
    }
}

/// 行内数据（图像/音频 base64）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiInlineData {
    pub mime_type: String,
    pub data: String,
}

/// 云端文件引用（Google Files API uploaded file）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFileData {
    pub mime_type: String,
    pub file_uri: String,
}

/// 工具调用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    pub name: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// 工具响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    #[serde(default)]
    pub response: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// `tools[]` 元素（Gemini 把多个函数声明包在一个 tool 对象的 `functionDeclarations`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_declarations: Vec<GeminiFunctionDeclaration>,

    /// Google Search grounding（预留透传）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_search: Option<serde_json::Value>,
    /// 代码执行（预留透传）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_execution: Option<serde_json::Value>,
}

/// 函数声明（类似 OpenAI 的 function tool）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// `toolConfig` 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiToolConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<GeminiFunctionCallingConfig>,
}

/// `functionCallingConfig.mode` = `AUTO` / `ANY` / `NONE`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionCallingConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_function_names: Vec<String>,
}

// ---------------------------------------------------------------------------
// Generation / Safety config
// ---------------------------------------------------------------------------

/// `generationConfig` 字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerationConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
    /// `gemini-2.5-pro-thinking` 等模型用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<GeminiThinkingConfig>,
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 思考预算控制。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiThinkingConfig {
    /// 思考 token 预算。0 = 禁用；-1 = 动态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,
    /// 是否在响应里返 thought summary。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
}

/// 单条安全设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSafetySetting {
    pub category: String,
    pub threshold: String,
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// `generateContent` / `streamGenerateContent` 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiChatResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<GeminiCandidate>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<GeminiPromptFeedback>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<GeminiUsageMetadata>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
}

/// 单个候选。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCandidate {
    #[serde(default)]
    pub index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<GeminiContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_ratings: Vec<GeminiSafetyRating>,
    /// grounding / citations 等，先 Value 透传。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<serde_json::Value>,
}

/// Prompt 被过滤时的反馈。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPromptFeedback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_ratings: Vec<GeminiSafetyRating>,
}

/// 安全评分。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSafetyRating {
    pub category: String,
    pub probability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
}

/// Usage 统计（含 cached content 字段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: i64,
    #[serde(default)]
    pub candidates_token_count: i64,
    #[serde(default)]
    pub total_token_count: i64,
    /// prompt cache 命中的 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    /// thinking token（2.5 系列）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<i64>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_request_roundtrip() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [
                {"role": "user", "parts": [{"text": "hi"}]}
            ]
        }))
        .unwrap();
        assert_eq!(req.contents.len(), 1);
        assert_eq!(req.contents[0].role.as_deref(), Some("user"));
        match &req.contents[0].parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "hi"),
            _ => panic!("expected Text part"),
        }
    }

    #[test]
    fn system_instruction_parses() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "systemInstruction": {"parts": [{"text": "you are helpful"}]}
        }))
        .unwrap();
        let sys = req.system_instruction.unwrap();
        assert_eq!(sys.parts.len(), 1);
        match &sys.parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "you are helpful"),
            _ => panic!("expected Text part"),
        }
    }

    #[test]
    fn function_call_and_response_parts() {
        let fc: GeminiPart = serde_json::from_value(serde_json::json!({
            "functionCall": {"name": "weather", "args": {"city": "NYC"}}
        }))
        .unwrap();
        match fc {
            GeminiPart::FunctionCall { function_call } => {
                assert_eq!(function_call.name, "weather");
                assert_eq!(function_call.args["city"], "NYC");
            }
            _ => panic!("expected FunctionCall"),
        }

        let fr: GeminiPart = serde_json::from_value(serde_json::json!({
            "functionResponse": {"name": "weather", "response": {"temp": "72F"}}
        }))
        .unwrap();
        match fr {
            GeminiPart::FunctionResponse { function_response } => {
                assert_eq!(function_response.name, "weather");
            }
            _ => panic!("expected FunctionResponse"),
        }
    }

    #[test]
    fn inline_data_camel_case_roundtrip() {
        let part: GeminiPart = serde_json::from_value(serde_json::json!({
            "inlineData": {"mimeType": "image/png", "data": "BASE64..."}
        }))
        .unwrap();
        match part {
            GeminiPart::InlineData { inline_data } => {
                assert_eq!(inline_data.mime_type, "image/png");
                assert_eq!(inline_data.data, "BASE64...");
            }
            _ => panic!("expected InlineData"),
        }
    }

    #[test]
    fn tools_function_declarations() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "tools": [{
                "functionDeclarations": [
                    {"name": "weather", "description": "...", "parameters": {"type": "object"}}
                ]
            }]
        }))
        .unwrap();
        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.tools[0].function_declarations.len(), 1);
        assert_eq!(req.tools[0].function_declarations[0].name, "weather");
    }

    #[test]
    fn generation_config_snake_to_camel() {
        let cfg = GeminiGenerationConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            max_output_tokens: Some(1024),
            ..Default::default()
        };
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(v["temperature"], 0.7);
        assert_eq!(v["topP"], 0.9);
        assert_eq!(v["topK"], 40);
        assert_eq!(v["maxOutputTokens"], 1024);
    }

    #[test]
    fn response_candidates_and_usage() {
        let resp: GeminiChatResponse = serde_json::from_value(serde_json::json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "hello"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 7,
                "totalTokenCount": 12,
                "cachedContentTokenCount": 0
            }
        }))
        .unwrap();
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(resp.candidates[0].finish_reason.as_deref(), Some("STOP"));
        let usage = resp.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, 5);
        assert_eq!(usage.total_token_count, 12);
    }

    #[test]
    fn thinking_config_roundtrip() {
        let cfg: GeminiThinkingConfig = serde_json::from_value(serde_json::json!({
            "thinkingBudget": 128,
            "includeThoughts": true
        }))
        .unwrap();
        assert_eq!(cfg.thinking_budget, Some(128));
        assert_eq!(cfg.include_thoughts, Some(true));
    }
}
