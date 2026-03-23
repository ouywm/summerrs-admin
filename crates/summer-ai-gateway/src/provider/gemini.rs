use super::ProviderAdapter;

/// Gemini 协议转换适配器
pub struct GeminiAdapter;

impl ProviderAdapter for GeminiAdapter {
    fn build_request(
        &self,
        _client: &reqwest::Client,
        _base_url: &str,
        _api_key: &str,
        _req: &crate::types::chat::ChatCompletionRequest,
        _actual_model: &str,
    ) -> anyhow::Result<reqwest::RequestBuilder> {
        todo!()
    }

    fn parse_response(
        &self,
        _body: bytes::Bytes,
        _model: &str,
    ) -> anyhow::Result<crate::types::chat::ChatCompletionResponse> {
        todo!()
    }

    fn parse_stream(
        &self,
        _stream: reqwest::Response,
        _model: &str,
    ) -> anyhow::Result<
        futures::stream::BoxStream<
            'static,
            anyhow::Result<crate::types::chat::ChatCompletionChunk>,
        >,
    > {
        todo!()
    }
}
