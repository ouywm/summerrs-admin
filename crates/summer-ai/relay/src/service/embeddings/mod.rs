use anyhow::Context;
use bytes::Bytes;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_core::provider::{ProviderKind, ProviderRegistry};
use summer_ai_core::types::embedding::{
    EmbeddingRequest, EmbeddingResponse, estimate_input_tokens,
};
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel::ChannelStatus;
use summer_ai_model::entity::channel_account;
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::service::chat::{
    RelayChatContext, effective_base_url, extract_api_key, extract_upstream_request_id,
    provider_error_to_openai_response, provider_kind_from_channel_type, resolve_upstream_model,
    select_schedulable_account,
};
use crate::service::tracking::TrackingService;

#[derive(Clone, Service)]
pub struct EmbeddingsRelayService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    client: reqwest::Client,
    #[inject(component)]
    tracking: TrackingService,
}

impl EmbeddingsRelayService {
    pub async fn relay(
        &self,
        ctx: RelayChatContext,
        request: EmbeddingRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        ctx.token_info
            .ensure_endpoint_allowed("embeddings")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        ctx.token_info
            .ensure_model_allowed(&request.model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;

        let request_id = format!("req_{}", Uuid::new_v4().simple());
        let started_at = std::time::Instant::now();
        let tracking = &self.tracking;

        let tracked_request = match tracking
            .create_embeddings_request(
                &request_id,
                &ctx.token_info,
                &request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            )
            .await
        {
            Ok(model) => Some(model),
            Err(error) => {
                tracing::warn!(request_id, error = %error, "failed to create request tracking row");
                None
            }
        };

        let target = match self.resolve_target(&ctx.token_info.group, &request).await {
            Ok(target) => target,
            Err(error) => {
                let openai_error = match error {
                    ApiErrors::NotFound(message) => {
                        OpenAiErrorResponse::model_not_available(message)
                    }
                    other => OpenAiErrorResponse::from_api_error(&other),
                };
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        None,
                        None,
                        None,
                        openai_error,
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
            let upstream_body = build_tracking_upstream_body(&request, &target.upstream_model);
            match tracking
                .create_embeddings_execution(
                    tracked_request.id,
                    &request_id,
                    &request,
                    target.channel.id,
                    target.account.id,
                    &target.upstream_model,
                    upstream_body,
                )
                .await
            {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::warn!(request_id, error = %error, "failed to create request_execution tracking row");
                    None
                }
            }
        } else {
            None
        };

        let provider = ProviderRegistry::embedding(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("embeddings endpoint is disabled")
        })?;

        let estimated_prompt_tokens = estimate_input_tokens(&request.input);
        let request_builder = match provider.build_embedding_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            &serde_json::to_value(&request).unwrap_or_else(|_| serde_json::json!({})),
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream embeddings request",
                            error,
                        ),
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let upstream_response = match self
            .send_upstream_embeddings(request_builder, target.provider_kind)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        None,
                        error,
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        if let Some(error) = upstream_response.error {
            return Err(self
                .finish_with_error(
                    tracking,
                    tracked_request.as_ref(),
                    tracked_execution.as_ref(),
                    Some(&target.upstream_model),
                    upstream_response.upstream_request_id.as_deref(),
                    error,
                    started_at.elapsed().as_millis() as i32,
                )
                .await);
        }

        let embedding_response = match provider.parse_embedding_response(
            upstream_response.body,
            &target.upstream_model,
            estimated_prompt_tokens,
        ) {
            Ok(response) => response,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        upstream_response.upstream_request_id.as_deref(),
                        OpenAiErrorResponse::internal_with(
                            "failed to parse upstream embeddings response",
                            error,
                        ),
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as i32;
        self.try_finish_request_success(
            tracking,
            tracked_request.as_ref(),
            &target.upstream_model,
            upstream_response.status_code,
            &embedding_response,
            duration_ms,
        )
        .await;
        self.try_finish_execution_success(
            tracking,
            tracked_execution.as_ref(),
            upstream_response.upstream_request_id.as_deref(),
            upstream_response.status_code,
            &embedding_response,
            duration_ms,
        )
        .await;

        Ok(Json::<EmbeddingResponse>(embedding_response).into_response())
    }

    async fn send_upstream_embeddings(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamEmbeddingsResponse, OpenAiErrorResponse> {
        let response = request_builder.send().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to call upstream provider", error)
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&headers);
        let body = response.bytes().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to read upstream response", error)
        })?;

        if status.is_success() {
            Ok(UpstreamEmbeddingsResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: None,
            })
        } else {
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamEmbeddingsResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: Some(provider_error_to_openai_response(status.as_u16(), &info)),
            })
        }
    }

    async fn resolve_target(
        &self,
        channel_group: &str,
        request: &EmbeddingRequest,
    ) -> ApiResult<ResolvedEmbeddingsTarget> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(channel_group))
            .filter(ability::Column::EndpointScope.eq("embeddings"))
            .filter(ability::Column::Model.eq(request.model.clone()))
            .filter(ability::Column::Enabled.eq(true))
            .order_by_desc(ability::Column::Priority)
            .order_by_desc(ability::Column::Weight)
            .order_by_desc(ability::Column::ChannelId)
            .all(&self.db)
            .await
            .context("查询模型能力失败")?;

        if abilities.is_empty() {
            return Err(ApiErrors::NotFound(format!(
                "model '{}' is not available",
                request.model
            )));
        }

        for ability in abilities {
            let Some(channel) = channel::Entity::find_by_id(ability.channel_id)
                .filter(channel::Column::DeletedAt.is_null())
                .one(&self.db)
                .await
                .context("查询渠道失败")?
            else {
                continue;
            };

            if channel.status != ChannelStatus::Enabled {
                continue;
            }

            let accounts = channel_account::Entity::find()
                .filter(channel_account::Column::ChannelId.eq(channel.id))
                .filter(channel_account::Column::DeletedAt.is_null())
                .order_by_desc(channel_account::Column::Priority)
                .order_by_desc(channel_account::Column::Weight)
                .order_by_desc(channel_account::Column::Id)
                .all(&self.db)
                .await
                .context("查询渠道账号失败")?;

            let Some(account) = select_schedulable_account(&accounts) else {
                continue;
            };

            let Some(api_key) = extract_api_key(&account) else {
                continue;
            };

            let provider_kind = provider_kind_from_channel_type(channel.channel_type)
                .ok_or_else(|| ApiErrors::BadRequest("unsupported channel type".to_string()))?;
            let upstream_model = resolve_upstream_model(&channel, &request.model);
            let base_url = effective_base_url(&channel, provider_kind);

            return Ok(ResolvedEmbeddingsTarget {
                channel,
                account,
                provider_kind,
                base_url,
                upstream_model,
                api_key,
            });
        }

        Err(ApiErrors::ServiceUnavailable(format!(
            "no available channel for model '{}'",
            request.model
        )))
    }

    async fn finish_with_error(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        tracked_execution: Option<&request_execution::Model>,
        upstream_model: Option<&str>,
        upstream_request_id: Option<&str>,
        openai_error: OpenAiErrorResponse,
        duration_ms: i32,
    ) -> OpenAiErrorResponse {
        let error_body =
            serde_json::to_value(&openai_error.error).unwrap_or_else(|_| serde_json::json!({}));
        self.try_finish_request_failure(
            tracking,
            tracked_request,
            upstream_model,
            &openai_error,
            Some(error_body.clone()),
            duration_ms,
        )
        .await;
        self.try_finish_execution_failure(
            tracking,
            tracked_execution,
            upstream_request_id,
            &openai_error,
            Some(error_body),
            duration_ms,
        )
        .await;
        openai_error
    }

    async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &EmbeddingResponse,
        duration_ms: i32,
    ) {
        if let Some(tracked_request) = tracked_request {
            if let Err(error) = tracking
                .finish_request_success(
                    tracked_request.id,
                    upstream_model,
                    response_status_code,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update request success tracking row");
            }
        }
    }

    async fn try_finish_request_failure(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: Option<&str>,
        openai_error: &OpenAiErrorResponse,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) {
        if let Some(tracked_request) = tracked_request {
            if let Err(error) = tracking
                .finish_request_failure(
                    tracked_request.id,
                    upstream_model,
                    openai_error.status as i32,
                    &openai_error.error.error.message,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update request failure tracking row");
            }
        }
    }

    async fn try_finish_execution_success(
        &self,
        tracking: &TrackingService,
        tracked_execution: Option<&request_execution::Model>,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        response_body: &EmbeddingResponse,
        duration_ms: i32,
    ) {
        if let Some(tracked_execution) = tracked_execution {
            if let Err(error) = tracking
                .finish_execution_success(
                    tracked_execution.id,
                    upstream_request_id,
                    response_status_code,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update request_execution success tracking row");
            }
        }
    }

    async fn try_finish_execution_failure(
        &self,
        tracking: &TrackingService,
        tracked_execution: Option<&request_execution::Model>,
        upstream_request_id: Option<&str>,
        openai_error: &OpenAiErrorResponse,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) {
        if let Some(tracked_execution) = tracked_execution {
            if let Err(error) = tracking
                .finish_execution_failure(
                    tracked_execution.id,
                    upstream_request_id,
                    openai_error.status as i32,
                    &openai_error.error.error.message,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update request_execution failure tracking row");
            }
        }
    }
}

#[derive(Clone)]
struct ResolvedEmbeddingsTarget {
    channel: channel::Model,
    account: channel_account::Model,
    provider_kind: ProviderKind,
    base_url: String,
    upstream_model: String,
    api_key: String,
}

struct UpstreamEmbeddingsResponse {
    status_code: i32,
    upstream_request_id: Option<String>,
    body: Bytes,
    error: Option<OpenAiErrorResponse>,
}

fn build_tracking_upstream_body(
    request: &EmbeddingRequest,
    upstream_model: &str,
) -> serde_json::Value {
    let mut body = serde_json::to_value(request).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "model".to_string(),
            serde_json::Value::String(upstream_model.to_string()),
        );
    }
    body
}

#[cfg(test)]
mod tests {
    use summer_ai_core::types::embedding::EmbeddingRequest;

    use super::build_tracking_upstream_body;

    #[test]
    fn build_tracking_upstream_body_overrides_model_and_keeps_input() {
        let request: EmbeddingRequest = serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": ["hello", "world"],
            "dimensions": 1024
        }))
        .expect("embedding request");

        let body = build_tracking_upstream_body(&request, "text-embedding-3-large");
        assert_eq!(body["model"], "text-embedding-3-large");
        assert_eq!(body["input"], serde_json::json!(["hello", "world"]));
        assert_eq!(body["dimensions"], 1024);
    }
}
