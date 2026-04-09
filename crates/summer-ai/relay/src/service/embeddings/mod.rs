use std::time::Instant;

use anyhow::Context;
use bytes::Bytes;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
use summer_ai_billing::service::engine::BillingEngine;
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

use crate::service::chat::{
    RelayChatContext, effective_base_url, extract_api_key, extract_upstream_request_id,
    provider_error_to_openai_response, provider_kind_from_channel_type, resolve_upstream_model,
    select_schedulable_account,
};
use crate::service::log::{FailureLogRecord, LogService, UsageLogRecord};
use crate::service::shared::request_prep::{
    PreparedRequestMeta, prepare_request_meta, try_create_tracked_request,
};
use crate::service::token::TokenInfo;
use crate::service::tracking::TrackingService;

#[derive(Clone, Service)]
pub struct EmbeddingsRelayService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    client: reqwest::Client,
    #[inject(component)]
    billing: BillingEngine,
    #[inject(component)]
    log: LogService,
    #[inject(component)]
    tracking: TrackingService,
}

impl EmbeddingsRelayService {
    fn error_context<'a>(
        &'a self,
        tracked_request: Option<&'a request::Model>,
        tracked_execution: Option<&'a request_execution::Model>,
        log_context: Option<&'a EmbeddingsLogContext>,
        billing: Option<&'a EmbeddingsBillingContext>,
        upstream_model: Option<&'a str>,
        started_at: &'a Instant,
    ) -> EmbeddingsErrorContext<'a> {
        EmbeddingsErrorContext {
            service: self,
            tracked_request,
            tracked_execution,
            log_context,
            billing,
            upstream_model,
            started_at,
        }
    }

    pub async fn relay(
        &self,
        ctx: RelayChatContext,
        request: EmbeddingRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedRequestMeta {
            request_id,
            started_at,
        } = prepare_request_meta(&ctx.token_info, "embeddings", &request.model)?;
        let tracking = &self.tracking;

        let tracked_request = try_create_tracked_request(
            &request_id,
            tracking.create_embeddings_request(
                &request_id,
                &ctx.token_info,
                &request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            ),
        )
        .await;
        let base_error_ctx = self.error_context(
            tracked_request.as_ref(),
            None,
            None,
            None,
            None,
            &started_at,
        );

        let target = match self.resolve_target(&ctx.token_info.group, &request).await {
            Ok(target) => target,
            Err(error) => {
                let openai_error = match error {
                    ApiErrors::NotFound(message) => {
                        OpenAiErrorResponse::model_not_available(message)
                    }
                    other => OpenAiErrorResponse::from_api_error(&other),
                };
                return Err(base_error_ctx.finish(None, openai_error).await);
            }
        };

        let billing = match self
            .prepare_embeddings_billing(&ctx.token_info, &request, &target)
            .await
        {
            Ok(billing) => billing,
            Err(error) => {
                return Err(self
                    .error_context(
                        tracked_request.as_ref(),
                        None,
                        None,
                        None,
                        Some(&target.upstream_model),
                        &started_at,
                    )
                    .finish(None, error)
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

        let log_context = build_embeddings_log_context(
            &ctx,
            target.channel.id,
            &target.channel.name,
            target.account.id,
            &target.account.name,
            tracked_execution
                .as_ref()
                .map(|model| model.id)
                .unwrap_or(0),
            &request.model,
        );
        let error_ctx = self.error_context(
            tracked_request.as_ref(),
            tracked_execution.as_ref(),
            Some(&log_context),
            Some(&billing),
            Some(&target.upstream_model),
            &started_at,
        );

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
                return Err(error_ctx
                    .finish(
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream embeddings request",
                            error,
                        ),
                    )
                    .await);
            }
        };

        let upstream_response = match self
            .send_upstream_embeddings(request_builder, target.provider_kind)
            .await
        {
            Ok(response) => response,
            Err(error) => return Err(error_ctx.finish(None, error).await),
        };

        if let Some(error) = upstream_response.error {
            return Err(error_ctx
                .finish(upstream_response.upstream_request_id.as_deref(), error)
                .await);
        }

        let embedding_response = match provider.parse_embedding_response(
            upstream_response.body,
            &target.upstream_model,
            estimated_prompt_tokens,
        ) {
            Ok(response) => response,
            Err(error) => {
                return Err(error_ctx
                    .finish(
                        upstream_response.upstream_request_id.as_deref(),
                        OpenAiErrorResponse::internal_with(
                            "failed to parse upstream embeddings response",
                            error,
                        ),
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
        self.try_settle_embeddings_billing_success(&request_id, &billing, &embedding_response)
            .await;
        self.try_record_embeddings_success_log(
            &ctx,
            &target,
            tracked_execution
                .as_ref()
                .map(|model| model.id)
                .unwrap_or(0),
            &billing,
            &request.model,
            &embedding_response,
            &request_id,
            upstream_response
                .upstream_request_id
                .as_deref()
                .unwrap_or_default(),
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
        log_context: Option<&EmbeddingsLogContext>,
        billing: Option<&EmbeddingsBillingContext>,
        upstream_model: Option<&str>,
        upstream_request_id: Option<&str>,
        openai_error: OpenAiErrorResponse,
        duration_ms: i32,
    ) -> OpenAiErrorResponse {
        self.try_refund_embeddings_billing("embeddings", billing)
            .await;
        let error_body =
            serde_json::to_value(&openai_error.error).unwrap_or_else(|_| serde_json::json!({}));
        self.try_record_embeddings_failure_log(
            log_context,
            billing,
            tracked_request
                .map(|tracked_request| tracked_request.request_id.as_str())
                .unwrap_or_default(),
            upstream_model.unwrap_or_default(),
            upstream_request_id.unwrap_or_default(),
            &openai_error,
            duration_ms,
        )
        .await;
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

    async fn prepare_embeddings_billing(
        &self,
        token_info: &crate::service::token::TokenInfo,
        request: &EmbeddingRequest,
        target: &ResolvedEmbeddingsTarget,
    ) -> Result<EmbeddingsBillingContext, OpenAiErrorResponse> {
        let price = self
            .billing
            .resolve_effective_price(target.channel.id, &request.model, "embeddings")
            .await
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        let group_ratio = self
            .billing
            .get_group_ratio(&token_info.group)
            .await
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        let estimated_tokens = estimate_input_tokens(&request.input);
        let pre_consumed = self
            .billing
            .pre_consume(
                token_info.token_id,
                token_info.unlimited_quota,
                estimated_tokens,
                price.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;

        Ok(EmbeddingsBillingContext {
            token_id: token_info.token_id,
            unlimited_quota: token_info.unlimited_quota,
            group_ratio,
            pre_consumed,
            price,
        })
    }

    async fn try_settle_embeddings_billing_success(
        &self,
        request_id: &str,
        billing: &EmbeddingsBillingContext,
        response: &EmbeddingResponse,
    ) {
        if let Err(error) = self
            .billing
            .post_consume(
                billing.token_id,
                billing.unlimited_quota,
                billing.pre_consumed,
                &response.usage,
                &billing.price,
                billing.group_ratio,
            )
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to settle embeddings billing");
        }
    }

    async fn try_refund_embeddings_billing(
        &self,
        request_id: &str,
        billing: Option<&EmbeddingsBillingContext>,
    ) {
        let Some(billing) = billing else {
            return;
        };

        if let Err(error) = self
            .billing
            .refund(billing.token_id, billing.pre_consumed)
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to refund embeddings billing reservation");
        }
    }

    async fn try_record_embeddings_success_log(
        &self,
        ctx: &RelayChatContext,
        target: &ResolvedEmbeddingsTarget,
        execution_id: i64,
        billing: &EmbeddingsBillingContext,
        requested_model: &str,
        response: &EmbeddingResponse,
        request_id: &str,
        upstream_request_id: &str,
        duration_ms: i32,
    ) {
        let quota = BillingEngine::calculate_actual_quota(
            &response.usage,
            &billing.price,
            billing.group_ratio,
        );

        let record = UsageLogRecord {
            channel_id: target.channel.id,
            channel_name: target.channel.name.clone(),
            account_id: target.account.id,
            account_name: target.account.name.clone(),
            execution_id,
            endpoint: "/v1/embeddings".into(),
            request_format: "openai/embeddings".into(),
            requested_model: requested_model.to_string(),
            upstream_model: target.upstream_model.clone(),
            model_name: billing.price.model_name.clone(),
            usage: response.usage.clone(),
            quota,
            cost_total: BillingEngine::calculate_cost_total(&response.usage, &billing.price),
            price_reference: billing.price.price_reference.clone(),
            elapsed_time: duration_ms,
            first_token_time: 0,
            is_stream: false,
            request_id: request_id.to_string(),
            upstream_request_id: upstream_request_id.to_string(),
            status_code: 200,
            client_ip: ctx.client_ip.clone(),
            user_agent: ctx.user_agent.clone(),
            content: String::new(),
        };

        if let Err(error) = self.log.record_usage(&ctx.token_info, record).await {
            tracing::warn!(request_id, error = %error, "failed to write embeddings usage log");
        }
    }

    async fn try_record_embeddings_failure_log(
        &self,
        log_context: Option<&EmbeddingsLogContext>,
        billing: Option<&EmbeddingsBillingContext>,
        request_id: &str,
        upstream_model: &str,
        upstream_request_id: &str,
        openai_error: &OpenAiErrorResponse,
        duration_ms: i32,
    ) {
        let (Some(log_context), Some(billing)) = (log_context, billing) else {
            return;
        };

        let record = FailureLogRecord {
            channel_id: log_context.channel_id,
            channel_name: log_context.channel_name.clone(),
            account_id: log_context.account_id,
            account_name: log_context.account_name.clone(),
            execution_id: log_context.execution_id,
            endpoint: "/v1/embeddings".into(),
            request_format: "openai/embeddings".into(),
            requested_model: log_context.requested_model.clone(),
            upstream_model: upstream_model.to_string(),
            model_name: billing.price.model_name.clone(),
            price_reference: billing.price.price_reference.clone(),
            elapsed_time: duration_ms,
            is_stream: false,
            request_id: request_id.to_string(),
            upstream_request_id: upstream_request_id.to_string(),
            status_code: openai_error.status as i32,
            client_ip: log_context.client_ip.clone(),
            user_agent: log_context.user_agent.clone(),
            content: openai_error.error.error.message.clone(),
        };

        if let Err(error) = self
            .log
            .record_failure(&log_context.token_info, record)
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to write embeddings failure log");
        }
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

#[derive(Clone)]
struct EmbeddingsLogContext {
    token_info: TokenInfo,
    channel_id: i64,
    channel_name: String,
    account_id: i64,
    account_name: String,
    execution_id: i64,
    requested_model: String,
    client_ip: String,
    user_agent: String,
}

struct EmbeddingsBillingContext {
    token_id: i64,
    unlimited_quota: bool,
    group_ratio: f64,
    pre_consumed: i64,
    price: ResolvedModelPrice,
}

struct EmbeddingsErrorContext<'a> {
    service: &'a EmbeddingsRelayService,
    tracked_request: Option<&'a request::Model>,
    tracked_execution: Option<&'a request_execution::Model>,
    log_context: Option<&'a EmbeddingsLogContext>,
    billing: Option<&'a EmbeddingsBillingContext>,
    upstream_model: Option<&'a str>,
    started_at: &'a Instant,
}

impl<'a> EmbeddingsErrorContext<'a> {
    async fn finish(
        &self,
        upstream_request_id: Option<&str>,
        error: OpenAiErrorResponse,
    ) -> OpenAiErrorResponse {
        self.service
            .finish_with_error(
                &self.service.tracking,
                self.tracked_request,
                self.tracked_execution,
                self.log_context,
                self.billing,
                self.upstream_model,
                upstream_request_id,
                error,
                self.started_at.elapsed().as_millis() as i32,
            )
            .await
    }
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

fn build_embeddings_log_context(
    ctx: &RelayChatContext,
    channel_id: i64,
    channel_name: &str,
    account_id: i64,
    account_name: &str,
    execution_id: i64,
    requested_model: &str,
) -> EmbeddingsLogContext {
    EmbeddingsLogContext {
        token_info: ctx.token_info.clone(),
        channel_id,
        channel_name: channel_name.to_string(),
        account_id,
        account_name: account_name.to_string(),
        execution_id,
        requested_model: requested_model.to_string(),
        client_ip: ctx.client_ip.clone(),
        user_agent: ctx.user_agent.clone(),
    }
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
