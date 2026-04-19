//! `AiToken` 提取器 —— handler 用它从 `Request::extensions` 取 [`AiTokenContext`]。
//!
//! 前置假设：请求已过 [`super::layer::AiAuthLayer`]。没过的话 extensions 里没东西，
//! extractor 返 401。

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};

use super::context::AiTokenContext;
use crate::error::RelayError;

/// 已鉴权 token 上下文的 extractor。
///
/// 用法：
/// ```ignore
/// #[post("/v1/chat/completions")]
/// async fn chat_completions(
///     AiToken(ctx): AiToken,
///     // ...
/// ) -> RelayResult<Response> { ... }
/// ```
pub struct AiToken(pub AiTokenContext);

impl<S> FromRequestParts<S> for AiToken
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AiTokenContext>()
            .cloned()
            .map(AiToken)
            .ok_or_else(|| {
                RelayError::Unauthenticated("missing AiTokenContext (layer not applied?)")
                    .into_response()
            })
    }
}

impl summer_web::aide::OperationInput for AiToken {}
