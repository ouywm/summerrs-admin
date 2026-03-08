use serde::Serialize;
use serde::de::DeserializeOwned;
use summer_web::axum::extract::{FromRequest, Request};
use summer_web::axum::response::IntoResponse;

/// JSON 包装器 — 替代 `axum::Json<T>`，同时用于请求提取和响应返回。
///
/// 与 `axum::Json<T>` 的区别：额外实现了 `Serialize`（`#[serde(transparent)]`），
/// 使得 `#[log]` 宏能够直接序列化成功响应体存入操作日志。
///
/// # 用法
///
/// ```rust,ignore
/// use common::response::Json;
///
/// // 作为响应
/// async fn get_user() -> ApiResult<Json<UserVo>> {
///     Ok(Json(vo))
/// }
///
/// // 作为请求体提取器
/// async fn create_user(Json(dto): Json<CreateDto>) -> ApiResult<()> {
///     Ok(())
/// }
/// ```
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Json<T>(pub T);

// ─── 请求提取器 ──────────────────────────────────────────────────────────────

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = <summer_web::axum::Json<T> as FromRequest<S>>::Rejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let summer_web::axum::Json(val) =
            summer_web::axum::Json::<T>::from_request(req, state).await?;
        Ok(Json(val))
    }
}

/// 请求体 OpenAPI 文档 — 委托给 `axum::Json<T>`
impl<T: schemars::JsonSchema> summer_web::aide::OperationInput for Json<T> {
    fn operation_input(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) {
        <summer_web::axum::Json<T> as summer_web::aide::OperationInput>::operation_input(
            ctx, operation,
        );
    }
}

// ─── 响应返回 ────────────────────────────────────────────────────────────────

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> summer_web::axum::response::Response {
        summer_web::axum::Json(self.0).into_response()
    }
}

/// 响应体 OpenAPI 文档 — 委托给 `axum::Json<T>`
impl<T: Serialize + schemars::JsonSchema> summer_web::aide::OperationOutput for Json<T> {
    type Inner = T;

    fn operation_response(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) -> Option<summer_web::aide::openapi::Response> {
        <summer_web::axum::Json<T> as summer_web::aide::OperationOutput>::operation_response(
            ctx, operation,
        )
    }

    fn inferred_responses(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) -> Vec<(
        Option<summer_web::aide::openapi::StatusCode>,
        summer_web::aide::openapi::Response,
    )> {
        <summer_web::axum::Json<T> as summer_web::aide::OperationOutput>::inferred_responses(
            ctx, operation,
        )
    }
}
