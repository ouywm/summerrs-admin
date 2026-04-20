//! `AiToken` 提取器 —— handler 用它从 `Request::extensions` 取 [`AiTokenContext`]。
//!
//! 前置假设：请求已过 [`super::layer::AiAuthLayer`]。没过的话 extensions 里没东西，
//! extractor 返 401（按当前路径推断的 flavor）。

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::Response;

use super::context::AiTokenContext;
use crate::error::{ErrorFlavor, RelayError};

/// 已鉴权 token 上下文的 extractor。
///
/// 用法：
/// ```ignore
/// #[post("/v1/chat/completions")]
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
        // layer 没挂或者 extensions 被清了——用 extensions 里的 flavor；若也没有，
        // 就按 URI 推断。
        let flavor = parts
            .extensions
            .get::<ErrorFlavor>()
            .copied()
            .unwrap_or_else(|| ErrorFlavor::from_path(parts.uri.path()));
        Err(
            RelayError::Unauthenticated("missing AiTokenContext (layer not applied?)")
                .into_response_with(flavor),
        )
    }
}

impl summer_web::aide::OperationInput for AiToken {}
