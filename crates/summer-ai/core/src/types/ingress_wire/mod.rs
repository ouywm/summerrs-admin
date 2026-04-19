//! 入口协议 wire 类型。
//!
//! 这些是**客户端发给我们的请求 / 我们返给客户端的响应**的原始格式，
//! 对应 `POST /v1/messages` (Claude) / `/v1beta/models/*/generateContent` (Gemini)
//! 等端点。
//!
//! 纯 struct + serde，**无转换逻辑**——ingress/egress converter 在 `relay/src/convert/`。
//!
//! # 列表
//!
//! - [`claude`] — Claude Messages API (`ClaudeMessagesRequest` / `ClaudeResponse` / `ClaudeStreamEvent`)
//! - [`gemini`] — Google Gemini GenerateContent (`GeminiGenerateContentRequest` / `GeminiChatResponse`)
//! - [`openai_responses`] — OpenAI Responses API (`OpenAIResponsesRequest` / `OpenAIResponsesResponse` / `OpenAIResponsesStreamEvent`)

pub mod claude;
pub mod gemini;
pub mod openai_responses;
