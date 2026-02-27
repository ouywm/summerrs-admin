use serde::de::DeserializeOwned;
use spring_web::axum::extract::{FromRequest, Request};
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
