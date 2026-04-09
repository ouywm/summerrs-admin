use std::time::Instant;

use anyhow::Context;
use bytes::Bytes;
use futures::StreamExt;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::provider::{ProviderKind, ProviderRegistry, ResponsesRuntimeMode};
use summer_ai_core::types::chat::ChatCompletionResponse;
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_core::types::responses::{
    ResponseInputTokensDetails, ResponseOutputTokensDetails, ResponseUsage, ResponsesRequest,
    ResponsesResponse, estimate_input_tokens,
};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::body::Body;
use summer_web::axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use summer_web::axum::http::{HeaderValue, StatusCode};
use summer_web::axum::response::{IntoResponse, Response};

use self::stream::{
    BridgeResponsesSseStreamArgs, NativeResponsesSseStreamArgs, ResponsesStreamCommonArgs,
    TrackedResponsesSseStream,
};
use crate::plugin::RelayStreamTaskTracker;
use crate::service::chat::{
    RelayChatContext, effective_base_url, extract_api_key, extract_upstream_request_id,
    provider_error_to_openai_response, provider_kind_from_channel_type, resolve_upstream_model,
    select_schedulable_account,
};
use crate::service::log::{LogService, UsageLogRecord};
use crate::service::shared::request_prep::{
    PreparedRequestMeta, prepare_request_meta, try_create_tracked_request,
};
use crate::service::token::TokenInfo;
use crate::service::tracking::TrackingService;
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel::ChannelStatus;
use summer_ai_model::entity::channel_account;
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};

mod stream;

#[derive(Clone, Service)]
pub struct ResponsesRelayService {
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
    #[inject(component)]
    stream_task_tracker: RelayStreamTaskTracker,
}

impl ResponsesRelayService {
    fn error_context<'a>(
        &'a self,
        tracked_request: Option<&'a request::Model>,
        tracked_execution: Option<&'a request_execution::Model>,
        log_context: Option<&'a ResponsesLogContext>,
        billing: Option<&'a ResponsesBillingContext>,
        upstream_model: Option<&'a str>,
        started_at: &'a Instant,
    ) -> ResponsesErrorContext<'a> {
        ResponsesErrorContext {
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
        request: ResponsesRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedRequestMeta {
            request_id,
            started_at,
        } = prepare_request_meta(&ctx.token_info, "responses", &request.model)?;
        let tracking = &self.tracking;

        let tracked_request = try_create_tracked_request(
            &request_id,
            tracking.create_responses_request(
                &request_id,
                &ctx.token_info,
                &request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            ),
        )
        .await;
        let base_error_ctx =
            self.error_context(tracked_request.as_ref(), None, None, None, None, &started_at);

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
            .prepare_responses_billing(&ctx.token_info, &request, &target)
            .await
        {
            Ok(billing) => billing,
            Err(error) => {
                return Err(
                    self.error_context(
                        tracked_request.as_ref(),
                        None,
                        None,
                        None,
                        Some(&target.upstream_model),
                        &started_at,
                    )
                    .finish(None, error)
                    .await,
                );
            }
        };

        let tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
            let upstream_body = build_tracking_upstream_body(&request, &target.upstream_model);
            match tracking
                .create_responses_execution(
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

        let log_context = build_responses_log_context(
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

        let provider = ProviderRegistry::responses(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("responses endpoint is disabled")
        })?;

        let request_builder = match provider.build_responses_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            &serde_json::to_value(&request).unwrap_or_else(|_| serde_json::json!({})),
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                return Err(
                    error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "failed to build upstream responses request",
                                error,
                            ),
                        )
                        .await,
                );
            }
        };

        if request.stream {
            let runtime_mode = provider.runtime_mode();
            let upstream_response = match self
                .send_upstream_responses_stream(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => return Err(error_ctx.finish(None, error).await),
            };

            let (response_status_code, upstream_request_id, response) = match upstream_response {
                UpstreamResponsesStreamResponse::Success {
                    status_code,
                    upstream_request_id,
                    response,
                } => (status_code, upstream_request_id, response),
                UpstreamResponsesStreamResponse::Failure {
                    upstream_request_id,
                    error,
                } => return Err(error_ctx.finish(upstream_request_id.as_deref(), error).await),
            };

            let common = ResponsesStreamCommonArgs {
                task_tracker: self.stream_task_tracker.clone(),
                tracking: Some(self.tracking.clone()),
                billing: Some(self.billing.clone()),
                billing_context: Some(billing.clone()),
                log: Some(self.log.clone()),
                log_context: Some(log_context.clone()),
                tracked_request_id: tracked_request.as_ref().map(|model| model.id),
                tracked_execution_id: tracked_execution.as_ref().map(|model| model.id),
                request_id: request_id.clone(),
                started_at,
                requested_model: request.model.clone(),
                upstream_model: target.upstream_model.clone(),
                upstream_request_id: upstream_request_id.clone(),
                response_status_code,
            };

            let stream = match runtime_mode {
                ResponsesRuntimeMode::Native => {
                    TrackedResponsesSseStream::native(NativeResponsesSseStreamArgs {
                        inner: Box::pin(
                            response
                                .bytes_stream()
                                .map(|chunk| chunk.map_err(anyhow::Error::from)),
                        ),
                        common,
                    })
                }
                ResponsesRuntimeMode::ChatBridge => {
                    let chat_provider =
                        ProviderRegistry::chat(target.provider_kind).ok_or_else(|| {
                            OpenAiErrorResponse::unsupported_endpoint(
                                "responses bridge requires chat provider support",
                            )
                        })?;
                    let chunk_stream = match chat_provider
                        .parse_chat_stream(response, &target.upstream_model)
                    {
                        Ok(stream) => stream,
                        Err(error) => {
                            return Err(
                                error_ctx
                                    .finish(
                                        upstream_request_id.as_deref(),
                                        OpenAiErrorResponse::internal_with(
                                            "failed to parse bridged responses stream",
                                            error,
                                        ),
                                    )
                                    .await,
                            );
                        }
                    };

                    TrackedResponsesSseStream::bridge(BridgeResponsesSseStreamArgs {
                        inner: chunk_stream,
                        common,
                    })
                }
            };

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::from_stream(stream))
                .expect("responses stream response");
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            if let Ok(value) = HeaderValue::from_str(&request_id) {
                response.headers_mut().insert("x-request-id", value);
            }
            if let Some(upstream_request_id) = upstream_request_id
                && let Ok(value) = HeaderValue::from_str(&upstream_request_id)
            {
                response
                    .headers_mut()
                    .insert("x-upstream-request-id", value);
            }

            return Ok(response);
        }

        let upstream_response = match self
            .send_upstream_responses(request_builder, target.provider_kind)
            .await
        {
            Ok(response) => response,
            Err(error) => return Err(error_ctx.finish(None, error).await),
        };

        if let Some(error) = upstream_response.error {
            return Err(
                error_ctx
                    .finish(upstream_response.upstream_request_id.as_deref(), error)
                    .await,
            );
        }

        let responses_response = match self.parse_responses_response(
            provider.runtime_mode(),
            target.provider_kind,
            upstream_response.body,
            &target.upstream_model,
        ) {
            Ok(response) => response,
            Err(error) => {
                return Err(
                    error_ctx
                        .finish(
                            upstream_response.upstream_request_id.as_deref(),
                            OpenAiErrorResponse::internal_with(
                                "failed to parse upstream responses response",
                                error,
                            ),
                        )
                        .await,
                );
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as i32;
        self.try_finish_request_success(
            tracking,
            tracked_request.as_ref(),
            &target.upstream_model,
            upstream_response.status_code,
            &responses_response,
            duration_ms,
        )
        .await;
        self.try_finish_execution_success(
            tracking,
            tracked_execution.as_ref(),
            upstream_response.upstream_request_id.as_deref(),
            upstream_response.status_code,
            &responses_response,
            duration_ms,
        )
        .await;
        self.try_settle_responses_billing_success(&request_id, &billing, &responses_response)
            .await;
        self.try_record_responses_success_log(
            &ctx,
            &target,
            tracked_execution
                .as_ref()
                .map(|model| model.id)
                .unwrap_or(0),
            &billing,
            &request.model,
            &responses_response,
            &request_id,
            upstream_response
                .upstream_request_id
                .as_deref()
                .unwrap_or_default(),
            duration_ms,
        )
        .await;

        Ok(Json::<ResponsesResponse>(responses_response).into_response())
    }

    async fn send_upstream_responses_stream(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamResponsesStreamResponse, OpenAiErrorResponse> {
        let response = request_builder.send().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to call upstream provider", error)
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&headers);

        if status.is_success() {
            Ok(UpstreamResponsesStreamResponse::Success {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                response,
            })
        } else {
            let body = response.bytes().await.map_err(|error| {
                OpenAiErrorResponse::internal_with("failed to read upstream response", error)
            })?;
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamResponsesStreamResponse::Failure {
                upstream_request_id,
                error: provider_error_to_openai_response(status.as_u16(), &info),
            })
        }
    }

    async fn send_upstream_responses(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamResponsesResponse, OpenAiErrorResponse> {
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
            Ok(UpstreamResponsesResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: None,
            })
        } else {
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamResponsesResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: Some(provider_error_to_openai_response(status.as_u16(), &info)),
            })
        }
    }

    fn parse_responses_response(
        &self,
        runtime_mode: ResponsesRuntimeMode,
        provider_kind: ProviderKind,
        body: Bytes,
        upstream_model: &str,
    ) -> anyhow::Result<ResponsesResponse> {
        match runtime_mode {
            ResponsesRuntimeMode::Native => serde_json::from_slice(&body).map_err(Into::into),
            ResponsesRuntimeMode::ChatBridge => {
                let provider = ProviderRegistry::chat(provider_kind)
                    .ok_or_else(|| anyhow::anyhow!("responses bridge requires chat provider"))?;
                let response = provider
                    .parse_chat_response(body, upstream_model)
                    .context("failed to parse bridged chat response")?;
                Ok(bridge_chat_response_to_responses_response(&response))
            }
        }
    }

    async fn resolve_target(
        &self,
        channel_group: &str,
        request: &ResponsesRequest,
    ) -> ApiResult<ResolvedResponsesTarget> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(channel_group))
            .filter(ability::Column::EndpointScope.eq("responses"))
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

            return Ok(ResolvedResponsesTarget {
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
        log_context: Option<&ResponsesLogContext>,
        billing: Option<&ResponsesBillingContext>,
        upstream_model: Option<&str>,
        upstream_request_id: Option<&str>,
        openai_error: OpenAiErrorResponse,
        duration_ms: i32,
    ) -> OpenAiErrorResponse {
        self.try_refund_responses_billing("responses", billing)
            .await;
        let error_body =
            serde_json::to_value(&openai_error.error).unwrap_or_else(|_| serde_json::json!({}));
        self.try_record_responses_failure_log(
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

    async fn prepare_responses_billing(
        &self,
        token_info: &crate::service::token::TokenInfo,
        request: &ResponsesRequest,
        target: &ResolvedResponsesTarget,
    ) -> Result<ResponsesBillingContext, OpenAiErrorResponse> {
        let price = self
            .billing
            .resolve_effective_price(target.channel.id, &request.model, "responses")
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

        Ok(ResponsesBillingContext {
            token_id: token_info.token_id,
            unlimited_quota: token_info.unlimited_quota,
            group_ratio,
            pre_consumed,
            price,
        })
    }

    async fn try_settle_responses_billing_success(
        &self,
        request_id: &str,
        billing: &ResponsesBillingContext,
        response: &ResponsesResponse,
    ) {
        let result = if let Some(usage) = response.usage.as_ref() {
            self.billing
                .post_consume(
                    billing.token_id,
                    billing.unlimited_quota,
                    billing.pre_consumed,
                    &usage.to_usage(),
                    &billing.price,
                    billing.group_ratio,
                )
                .await
        } else {
            self.billing
                .settle_pre_consumed(
                    billing.token_id,
                    billing.unlimited_quota,
                    billing.pre_consumed,
                )
                .await
        };

        if let Err(error) = result {
            tracing::warn!(request_id, error = %error, "failed to settle responses billing");
        }
    }

    async fn try_refund_responses_billing(
        &self,
        request_id: &str,
        billing: Option<&ResponsesBillingContext>,
    ) {
        let Some(billing) = billing else {
            return;
        };

        if let Err(error) = self
            .billing
            .refund(billing.token_id, billing.pre_consumed)
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to refund responses billing reservation");
        }
    }

    async fn try_record_responses_success_log(
        &self,
        ctx: &RelayChatContext,
        target: &ResolvedResponsesTarget,
        execution_id: i64,
        billing: &ResponsesBillingContext,
        requested_model: &str,
        response: &ResponsesResponse,
        request_id: &str,
        upstream_request_id: &str,
        duration_ms: i32,
    ) {
        let usage = response
            .usage
            .as_ref()
            .map(ResponseUsage::to_usage)
            .unwrap_or_default();
        let quota = response
            .usage
            .as_ref()
            .map_or(billing.pre_consumed, |usage| {
                BillingEngine::calculate_actual_quota(
                    &usage.to_usage(),
                    &billing.price,
                    billing.group_ratio,
                )
            });
        let cost_total = BillingEngine::calculate_cost_total(&usage, &billing.price);

        let record = UsageLogRecord {
            channel_id: target.channel.id,
            channel_name: target.channel.name.clone(),
            account_id: target.account.id,
            account_name: target.account.name.clone(),
            execution_id,
            endpoint: "/v1/responses".into(),
            request_format: "openai/responses".into(),
            requested_model: requested_model.to_string(),
            upstream_model: target.upstream_model.clone(),
            model_name: billing.price.model_name.clone(),
            usage,
            quota,
            cost_total,
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
            tracing::warn!(request_id, error = %error, "failed to write responses usage log");
        }
    }

    async fn try_record_responses_failure_log(
        &self,
        log_context: Option<&ResponsesLogContext>,
        billing: Option<&ResponsesBillingContext>,
        request_id: &str,
        upstream_model: &str,
        upstream_request_id: &str,
        openai_error: &OpenAiErrorResponse,
        duration_ms: i32,
    ) {
        let (Some(log_context), Some(billing)) = (log_context, billing) else {
            return;
        };

        let record = crate::service::log::FailureLogRecord {
            channel_id: log_context.channel_id,
            channel_name: log_context.channel_name.clone(),
            account_id: log_context.account_id,
            account_name: log_context.account_name.clone(),
            execution_id: log_context.execution_id,
            endpoint: "/v1/responses".into(),
            request_format: "openai/responses".into(),
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

        if let Err(error) = self.log.record_failure(&log_context.token_info, record).await {
            tracing::warn!(request_id, error = %error, "failed to write responses failure log");
        }
    }

    async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &ResponsesResponse,
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
        response_body: &ResponsesResponse,
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

fn bridge_chat_response_to_responses_response(
    response: &ChatCompletionResponse,
) -> ResponsesResponse {
    ResponsesResponse {
        id: response.id.clone(),
        object: "response".into(),
        created_at: response.created,
        model: response.model.clone(),
        status: "completed".into(),
        usage: Some(ResponseUsage {
            input_tokens: response.usage.prompt_tokens,
            output_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
            input_tokens_details: (response.usage.cached_tokens > 0).then_some(
                ResponseInputTokensDetails {
                    cached_tokens: response.usage.cached_tokens,
                },
            ),
            output_tokens_details: (response.usage.reasoning_tokens > 0).then_some(
                ResponseOutputTokensDetails {
                    reasoning_tokens: response.usage.reasoning_tokens,
                },
            ),
        }),
        output_text: response
            .choices
            .first()
            .and_then(|choice| choice.message.text_content())
            .map(ToOwned::to_owned),
        extra: serde_json::Map::new(),
    }
}

#[derive(Clone)]
struct ResolvedResponsesTarget {
    channel: channel::Model,
    account: channel_account::Model,
    provider_kind: ProviderKind,
    base_url: String,
    upstream_model: String,
    api_key: String,
}

struct UpstreamResponsesResponse {
    status_code: i32,
    upstream_request_id: Option<String>,
    body: Bytes,
    error: Option<OpenAiErrorResponse>,
}

enum UpstreamResponsesStreamResponse {
    Success {
        status_code: i32,
        upstream_request_id: Option<String>,
        response: reqwest::Response,
    },
    Failure {
        upstream_request_id: Option<String>,
        error: OpenAiErrorResponse,
    },
}

#[derive(Clone)]
struct ResponsesBillingContext {
    token_id: i64,
    unlimited_quota: bool,
    group_ratio: f64,
    pre_consumed: i64,
    price: ResolvedModelPrice,
}

#[derive(Clone)]
pub(crate) struct ResponsesLogContext {
    pub(crate) token_info: TokenInfo,
    pub(crate) channel_id: i64,
    pub(crate) channel_name: String,
    pub(crate) account_id: i64,
    pub(crate) account_name: String,
    pub(crate) execution_id: i64,
    pub(crate) requested_model: String,
    pub(crate) client_ip: String,
    pub(crate) user_agent: String,
}

struct ResponsesErrorContext<'a> {
    service: &'a ResponsesRelayService,
    tracked_request: Option<&'a request::Model>,
    tracked_execution: Option<&'a request_execution::Model>,
    log_context: Option<&'a ResponsesLogContext>,
    billing: Option<&'a ResponsesBillingContext>,
    upstream_model: Option<&'a str>,
    started_at: &'a Instant,
}

impl<'a> ResponsesErrorContext<'a> {
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
    request: &ResponsesRequest,
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

fn stream_error_status_code(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| match error.info.kind {
            summer_ai_core::provider::ProviderErrorKind::InvalidRequest => 400,
            summer_ai_core::provider::ProviderErrorKind::Authentication => 401,
            summer_ai_core::provider::ProviderErrorKind::RateLimit => 429,
            summer_ai_core::provider::ProviderErrorKind::Server
            | summer_ai_core::provider::ProviderErrorKind::Api => 502,
        })
        .unwrap_or(0)
}

fn stream_error_message(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| error.info.message.clone())
        .unwrap_or_else(|| error.to_string())
}

fn build_responses_log_context(
    ctx: &RelayChatContext,
    channel_id: i64,
    channel_name: &str,
    account_id: i64,
    account_name: &str,
    execution_id: i64,
    _requested_model: &str,
) -> ResponsesLogContext {
    ResponsesLogContext {
        token_info: ctx.token_info.clone(),
        channel_id,
        channel_name: channel_name.to_string(),
        account_id,
        account_name: account_name.to_string(),
        execution_id,
        requested_model: _requested_model.to_string(),
        client_ip: ctx.client_ip.clone(),
        user_agent: ctx.user_agent.clone(),
    }
}

#[cfg(test)]
mod tests {
    use summer_ai_core::types::chat::ChatCompletionResponse;

    use super::bridge_chat_response_to_responses_response;

    #[test]
    fn bridge_chat_response_to_responses_response_preserves_usage_and_text() {
        let chat: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl_123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-5.4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello from chat"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 11,
                "completion_tokens": 7,
                "total_tokens": 18,
                "cached_tokens": 2,
                "reasoning_tokens": 3
            }
        }))
        .expect("chat response");

        let bridged = bridge_chat_response_to_responses_response(&chat);
        assert_eq!(bridged.object, "response");
        assert_eq!(bridged.model, "gpt-5.4");
        assert_eq!(bridged.output_text.as_deref(), Some("hello from chat"));
        assert_eq!(bridged.status, "completed");

        let usage = bridged.usage.expect("usage");
        assert_eq!(usage.input_tokens, 11);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.total_tokens, 18);
        assert_eq!(
            usage
                .input_tokens_details
                .expect("input details")
                .cached_tokens,
            2
        );
        assert_eq!(
            usage
                .output_tokens_details
                .expect("output details")
                .reasoning_tokens,
            3
        );
    }
}
