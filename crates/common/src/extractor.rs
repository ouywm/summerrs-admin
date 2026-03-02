use serde::de::DeserializeOwned;
use spring_web::axum::extract::{FromRequest, FromRequestParts, Request};
use spring_web::axum::http::request::Parts;
use spring_web::axum::response::IntoResponse;
use spring_web::axum::Json;
use validator::Validate;

use crate::error::ApiErrors;

/// 自定义 JSON 提取器，验证失败时返回 ApiErrors
pub struct ValidatedJson<T>(pub T);

impl<T> std::ops::Deref for ValidatedJson<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for ValidatedJson<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// 提取验证错误的第一条消息（去掉字段名前缀）
fn extract_first_error_message(errors: &validator::ValidationErrors) -> String {
    for (_, field_errors) in errors.field_errors() {
        if let Some(first_error) = field_errors.first() {
            if let Some(msg) = &first_error.message {
                return msg.to_string();
            }
        }
    }
    "验证失败".to_string()
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiErrors;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(data) = Json::<T>::from_request(req, state)
            .await
            .map_err(|_| ApiErrors::BadRequest("请求数据无法解析".to_string()))?;

        data.validate()
            .map_err(|e| ApiErrors::ValidationFailed(extract_first_error_message(&e)))?;

        Ok(ValidatedJson(data))
    }
}

/// ValidatedJson 对 OpenAPI 文档：生成请求体 schema（委托给 Json<T>）
impl<T: schemars::JsonSchema> spring_web::aide::OperationInput for ValidatedJson<T> {
    fn operation_input(
        ctx: &mut spring_web::aide::generate::GenContext,
        operation: &mut spring_web::aide::openapi::Operation,
    ) {
        <spring_web::axum::Json<T> as spring_web::aide::OperationInput>::operation_input(ctx, operation);
    }
}

/// 登录用户 ID 提取器
///
/// 从请求 Extensions 中提取 sa-token 写入的 login_id（String）。
/// 未登录时返回 401。
pub struct LoginIdExtractor(pub String);

impl<S: Send + Sync> FromRequestParts<S> for LoginIdExtractor {
    type Rejection = spring_web::axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<String>() {
            Some(login_id) => Ok(LoginIdExtractor(login_id.clone())),
            None => Err((
                spring_web::axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "code": 401,
                    "message": "未登录或登录已过期"
                })),
            )
                .into_response()),
        }
    }
}

/// LoginIdExtractor 对 OpenAPI 文档透明（不生成参数描述）
impl spring_web::aide::OperationInput for LoginIdExtractor {}

/// 客户端 IP 提取器（包装 axum_client_ip::ClientIp）
///
/// 因 axum_client_ip::ClientIp 来自外部 crate，无法为其实现 OperationInput
/// 此包装类型委托提取逻辑给原版，同时实现 OperationInput 使其对 OpenAPI 文档透明。
pub struct ClientIp(pub std::net::IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = <axum_client_ip::ClientIp as spring_web::axum::extract::FromRequestParts<S>>::Rejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum_client_ip::ClientIp(ip) =
            axum_client_ip::ClientIp::from_request_parts(parts, state).await?;
        Ok(ClientIp(ip))
    }
}

/// ClientIp 对 OpenAPI 文档透明（不生成参数描述）
impl spring_web::aide::OperationInput for ClientIp {}
