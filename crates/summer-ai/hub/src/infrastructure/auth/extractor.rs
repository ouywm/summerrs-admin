use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;

use crate::service::token::TokenInfo;

/// 已通过基础鉴权的 Token 提取器。
pub struct AiToken(pub TokenInfo);

impl std::ops::Deref for AiToken {
    type Target = TokenInfo;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AiToken {
    type Rejection = OpenAiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .extensions
            .get::<TokenInfo>()
            .cloned()
            .ok_or_else(|| OpenAiErrorResponse::invalid_api_key("缺少有效的 API Key"))?;
        Ok(AiToken(token))
    }
}

impl summer_web::aide::OperationInput for AiToken {}

/// 可选 Token 提取器。
pub struct OptionalAiToken(pub Option<AiToken>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalAiToken {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(OptionalAiToken(
            parts.extensions.get::<TokenInfo>().cloned().map(AiToken),
        ))
    }
}

impl summer_web::aide::OperationInput for OptionalAiToken {}
