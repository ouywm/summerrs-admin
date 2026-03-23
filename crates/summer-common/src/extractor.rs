use serde::de::DeserializeOwned;
use summer_web::axum::Json;
use summer_web::axum::extract::{FromRequest, FromRequestParts, Request};
use summer_web::axum::http::request::Parts;
use validator::Validate;

use crate::error::ApiErrors;

// ─── Query ───────────────────────────────────────────────────────────────────

/// 自定义 Query 提取器，反序列化失败时返回 ProblemDetails 格式错误
pub struct Query<T>(pub T);

impl<T> std::ops::Deref for Query<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiErrors;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        summer_web::axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|summer_web::axum::extract::Query(v)| Query(v))
            .map_err(|e| ApiErrors::BadRequest(strip_rejection_prefix(&e.to_string())))
    }
}

/// Query 对 OpenAPI：委托给 axum::Query<T> 生成 query 参数 schema
impl<T: schemars::JsonSchema> summer_web::aide::OperationInput for Query<T> {
    fn operation_input(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) {
        <summer_web::axum::extract::Query<T> as summer_web::aide::OperationInput>::operation_input(
            ctx, operation,
        );
    }
}

// ─── Path ────────────────────────────────────────────────────────────────────

/// 自定义 Path 提取器，反序列化失败时返回 ProblemDetails 格式错误
pub struct Path<T>(pub T);

impl<T> std::ops::Deref for Path<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiErrors;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        summer_web::axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|summer_web::axum::extract::Path(v)| Path(v))
            .map_err(|e| ApiErrors::BadRequest(strip_rejection_prefix(&e.to_string())))
    }
}

/// Path 对 OpenAPI：委托给 axum::Path<T> 生成 path 参数 schema
impl<T: schemars::JsonSchema> summer_web::aide::OperationInput for Path<T> {
    fn operation_input(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) {
        <summer_web::axum::extract::Path<T> as summer_web::aide::OperationInput>::operation_input(
            ctx, operation,
        );
    }
}

// ─── 工具函数 ────────────────────────────────────────────────────────────────

/// 去掉 axum rejection 消息中的前缀（如 "Failed to deserialize query string: "）
/// 只保留实际的错误描述
fn strip_rejection_prefix(msg: &str) -> String {
    msg.find(": ")
        .map(|i| msg[i + 2..].to_string())
        .unwrap_or_else(|| msg.to_string())
}

// ─── ValidatedJson ───────────────────────────────────────────────────────────

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
        if let Some(first_error) = field_errors.first()
            && let Some(msg) = &first_error.message
        {
            return msg.to_string();
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
impl<T: schemars::JsonSchema> summer_web::aide::OperationInput for ValidatedJson<T> {
    fn operation_input(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) {
        <summer_web::axum::Json<T> as summer_web::aide::OperationInput>::operation_input(
            ctx, operation,
        );
    }
}

// ─── ClientIp ────────────────────────────────────────────────────────────────

/// 客户端 IP 提取器（包装 axum_client_ip::ClientIp）
///
/// 因 axum_client_ip::ClientIp 来自外部 crate，无法为其实现 OperationInput
/// 此包装类型委托提取逻辑给原版，同时实现 OperationInput 使其对 OpenAPI 文档透明。
pub struct ClientIp(pub std::net::IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection =
        <axum_client_ip::ClientIp as summer_web::axum::extract::FromRequestParts<S>>::Rejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum_client_ip::ClientIp(ip) =
            axum_client_ip::ClientIp::from_request_parts(parts, state).await?;
        Ok(ClientIp(ip))
    }
}

/// ClientIp 对 OpenAPI 文档透明（不生成参数描述）
impl summer_web::aide::OperationInput for ClientIp {}

// ─── Multipart ──────────────────────────────────────────────────────────────

/// Multipart 提取器（包装 axum::extract::Multipart）
///
/// axum::extract::Multipart 来自 axum，未实现 aide 的 OperationInput，
/// 此包装类型委托提取逻辑给原版，同时实现 OperationInput 使其对 OpenAPI 文档透明。
pub struct Multipart(pub summer_web::axum::extract::Multipart);

impl<S: Send + Sync> FromRequest<S> for Multipart {
    type Rejection = <summer_web::axum::extract::Multipart as FromRequest<S>>::Rejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let multipart = summer_web::axum::extract::Multipart::from_request(req, state).await?;
        Ok(Multipart(multipart))
    }
}

/// Multipart 对 OpenAPI 文档透明（不生成参数描述）
impl summer_web::aide::OperationInput for Multipart {}
