pub mod anthropic;
pub mod gemini;
pub mod openai;

use anyhow::Result;
use futures::stream::BoxStream;

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

/// 后端 Provider 适配器 trait
///
/// 每种 LLM 后端实现此 trait 来做协议转换：
/// - OpenAI 兼容的后端（DeepSeek、Groq 等）直接透传
/// - Claude、Gemini 等需要格式转换
pub trait ProviderAdapter: Send + Sync {
    /// 构建发往后端的 HTTP 请求
    fn build_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    /// 将后端非流式响应转为 OpenAI 格式
    fn parse_response(&self, body: bytes::Bytes, model: &str)
        -> Result<ChatCompletionResponse>;

    /// 将后端流式 SSE 转为 OpenAI chunk 流
    fn parse_stream(
        &self,
        stream: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;
}

/// 根据渠道类型获取对应的适配器
pub fn get_adapter(_channel_type: i16) -> &'static dyn ProviderAdapter {
    // TODO: match channel_type { 1 => &openai::ADAPTER, 3 => &anthropic::ADAPTER, ... }
    todo!()
}
