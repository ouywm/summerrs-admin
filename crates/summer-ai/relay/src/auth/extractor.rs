//! `AiToken` 提取器 —— handler 用它从 `Request::extensions` 取 [`AiTokenContext`]。
//!
//! 前置假设:请求已过 [`super::api_key_strategy::ApiKeyStrategy`]。没过的话
//! extensions 里没东西,extractor 走 fallback 路径返 401。

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::Response;

use super::context::AiTokenContext;
use crate::error::{ErrorFlavor, RelayError};

/// 已鉴权 token 上下文的 extractor。
///
/// 用法:
/// ```ignore
/// #[post("/v1/chat/completions", group = "summer-ai-relay::openai")]
/// async fn chat_completions(
///     AiToken(ctx): AiToken,
///     // ...
/// ) -> OpenAIResult<Response> { ... }
/// ```
pub struct AiToken(pub AiTokenContext);

impl<S> FromRequestParts<S> for AiToken
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(ctx) = parts.extensions.get::<AiTokenContext>().cloned() {
            return Ok(AiToken(ctx));
        }
        // 走到这里说明对应子 router 漏挂了 `ApiKeyStrategy` —— 属于配置错误,
        // 生产不应触发。flavor 用 OpenAI 兜底;真正的 flavor 由 router 静态绑定,
        // 正常路径上 401 走的是 strategy 自己的 `into_response_with`。
        Err(
            RelayError::Unauthenticated("missing AiTokenContext (auth layer not applied?)")
                .into_response_with(ErrorFlavor::OpenAI),
        )
    }
}

impl summer_web::aide::OperationInput for AiToken {}
