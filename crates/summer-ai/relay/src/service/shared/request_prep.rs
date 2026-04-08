use std::time::Instant;

use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_common::error::ApiResult;
use uuid::Uuid;

use crate::service::token::TokenInfo;

#[derive(Debug, Clone)]
pub(crate) struct PreparedRequestMeta {
    pub(crate) request_id: String,
    pub(crate) started_at: Instant,
}

impl PreparedRequestMeta {
    pub(crate) fn new() -> Self {
        Self {
            request_id: format!("req_{}", Uuid::new_v4().simple()),
            started_at: Instant::now(),
        }
    }
}

pub(crate) fn prepare_request_meta(
    token_info: &TokenInfo,
    endpoint_scope: &str,
    model: &str,
) -> Result<PreparedRequestMeta, OpenAiErrorResponse> {
    token_info
        .ensure_endpoint_allowed(endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_info
        .ensure_model_allowed(model)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;

    Ok(PreparedRequestMeta::new())
}

pub(crate) async fn try_create_tracked_request<T, F>(request_id: &str, create: F) -> Option<T>
where
    F: std::future::Future<Output = ApiResult<T>>,
{
    match create.await {
        Ok(model) => Some(model),
        Err(error) => {
            tracing::warn!(request_id, error = %error, "failed to create request tracking row");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PreparedRequestMeta, prepare_request_meta, try_create_tracked_request};
    use crate::service::token::TokenInfo;
    use summer_common::error::ApiErrors;

    fn token_info() -> TokenInfo {
        TokenInfo {
            token_id: 1,
            user_id: 2,
            name: "demo".into(),
            group: "default".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: vec!["gpt-4o".into()],
            endpoint_scopes: vec!["chat".into()],
        }
    }

    #[test]
    fn prepared_request_meta_uses_req_prefix() {
        let meta = PreparedRequestMeta::new();
        assert!(meta.request_id.starts_with("req_"));
    }

    #[test]
    fn prepare_request_meta_checks_scope_and_model() {
        let token = token_info();
        assert!(prepare_request_meta(&token, "chat", "gpt-4o").is_ok());
        assert!(prepare_request_meta(&token, "embeddings", "gpt-4o").is_err());
        assert!(prepare_request_meta(&token, "chat", "gpt-4.1").is_err());
    }

    #[tokio::test]
    async fn try_create_tracked_request_returns_none_on_failure() {
        let tracked = try_create_tracked_request::<i32, _>("req_test", async {
            Err(ApiErrors::Internal(anyhow::anyhow!("boom")))
        })
        .await;
        assert!(tracked.is_none());
    }
}
