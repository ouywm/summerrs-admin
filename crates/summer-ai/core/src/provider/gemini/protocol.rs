#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiRequest {
    pub contents: Vec<GeminiRequestContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiSystemInstruction>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiRequestContent {
    pub role: String,
    pub parts: Vec<GeminiRequestPart>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiRequestPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "inlineData", skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<GeminiInlineData>,
    #[serde(rename = "fileData", skip_serializing_if = "Option::is_none")]
    pub file_data: Option<GeminiFileData>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    pub function_call: Option<GeminiFunctionCall>,
    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    pub function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiInlineData {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiFileData {
    pub mime_type: String,
    pub file_uri: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GeminiFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiToolConfig {
    pub function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiFunctionCallingConfig {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiFunctionDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiSystemInstruction {
    pub parts: Vec<GeminiTextPart>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiTextPart {
    pub text: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiEmbedContentRequest {
    pub model: String,
    pub content: GeminiEmbedContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<i32>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiBatchEmbedContentsRequest {
    pub requests: Vec<GeminiEmbedContentRequest>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct GeminiEmbedContent {
    pub parts: Vec<GeminiTextPart>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiResponse {
    #[serde(default)]
    pub candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    pub usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiCandidate {
    #[serde(default)]
    pub content: Option<GeminiResponseContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct GeminiResponseContent {
    #[serde(default)]
    pub parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct GeminiResponsePart {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "functionCall", default)]
    pub function_call: Option<GeminiFunctionCall>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: i32,
    #[serde(default)]
    pub candidates_token_count: i32,
    #[serde(default)]
    pub total_token_count: i32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiEmbedContentResponse {
    #[serde(default)]
    pub embedding: Option<GeminiEmbeddingValue>,
    #[serde(default)]
    pub embeddings: Vec<GeminiEmbeddingValue>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct GeminiEmbeddingValue {
    #[serde(default)]
    pub values: Vec<f32>,
}
