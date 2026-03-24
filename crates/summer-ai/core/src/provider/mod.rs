use anyhow::Result;
use bytes::Bytes;
use futures::stream::BoxStream;

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

mod openai;

pub use openai::OpenAiAdapter;

/// Provider 适配器 trait
///
/// 所有方法均为同步；异步由流本身承载。
/// 结构体统一定义在 `crate::types`，此处不定义业务结构体。
pub trait ProviderAdapter: Send + Sync {
    /// 构建上游 HTTP 请求
    fn build_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    /// 解析非流式响应
    fn parse_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse>;

    /// 解析流式响应，返回 chunk 流
    fn parse_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;
}

/// 根据渠道类型获取对应适配器（零状态，全局静态实例）
pub fn get_adapter(channel_type: i16) -> &'static dyn ProviderAdapter {
    static OPENAI: OpenAiAdapter = OpenAiAdapter;

    match channel_type {
        1 => &OPENAI, // OpenAI / OpenAI 兼容
        _ => &OPENAI, // 默认 OpenAI 兼容
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_adapter_openai() {
        let adapter = get_adapter(1);
        // 验证返回的是合法的 trait object
        let _ = format!("{:p}", adapter);
    }

    #[test]
    fn get_adapter_unknown_defaults_to_openai() {
        let a = get_adapter(1);
        let b = get_adapter(999);
        // 未知类型回退到 OpenAI，指向同一个静态实例
        assert!(std::ptr::eq(a, b));
    }
}
