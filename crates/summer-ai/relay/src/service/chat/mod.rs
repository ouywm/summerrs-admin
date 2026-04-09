use std::time::Instant;

use summer::plugin::Service;
use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::provider::{ChatProvider, ProviderKind, ProviderRegistry};
use summer_ai_core::types::chat::{ChatCompletionRequest, ChatCompletionResponse};
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::{IntoResponse, Response};

use crate::plugin::RelayStreamTaskTracker;
use crate::service::log::{FailureLogRecord, LogService, UsageLogRecord};
use crate::service::shared::relay::{
    RELAY_MAX_UPSTREAM_ATTEMPTS, ResolvedRelayTarget, advance_relay_retry,
    build_retry_attempt_payload, complete_relay_retry_success, extract_upstream_request_id,
    is_retryable_upstream_error, provider_error_to_openai_response, resolve_relay_target,
};
use crate::service::shared::request_prep::{
    PreparedRequestMeta, prepare_request_meta, try_create_tracked_request,
};
use crate::service::token::TokenInfo;
use crate::service::tracking::TrackingService;

mod stream;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct RelayChatContext {
    pub token_info: TokenInfo,
    pub client_ip: String,
    pub user_agent: String,
    pub request_headers: HeaderMap,
}

type ResolvedChatTarget = ResolvedRelayTarget;

#[derive(Clone)]
pub(crate) struct ChatBillingContext {
    token_id: i64,
    unlimited_quota: bool,
    group_ratio: f64,
    pre_consumed: i64,
    estimated_prompt_tokens: i32,
    price: ResolvedModelPrice,
}

#[derive(Clone)]
pub(crate) struct ChatLogContext {
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

struct PreparedChatRelay {
    request_id: String,
    started_at: Instant,
    tracked_request: Option<request::Model>,
    tracked_execution: Option<request_execution::Model>,
    billing: ChatBillingContext,
    log_context: ChatLogContext,
    target: ResolvedChatTarget,
    provider: &'static dyn ChatProvider,
    request_builder: reqwest::RequestBuilder,
}

struct ChatErrorContext<'a> {
    service: &'a ChatRelayService,
    tracked_request: Option<&'a request::Model>,
    tracked_execution: Option<&'a request_execution::Model>,
    log_context: Option<&'a ChatLogContext>,
    billing: Option<&'a ChatBillingContext>,
    upstream_model: Option<&'a str>,
    started_at: &'a Instant,
}

impl<'a> ChatErrorContext<'a> {
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

struct UpstreamChatResponse {
    status_code: i32,
    upstream_request_id: Option<String>,
    body: bytes::Bytes,
    error: Option<OpenAiErrorResponse>,
}

enum UpstreamChatStreamResponse {
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

#[derive(Clone, Service)]
pub struct ChatRelayService {
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

impl ChatRelayService {
    fn error_context<'a>(
        &'a self,
        tracked_request: Option<&'a request::Model>,
        tracked_execution: Option<&'a request_execution::Model>,
        log_context: Option<&'a ChatLogContext>,
        billing: Option<&'a ChatBillingContext>,
        upstream_model: Option<&'a str>,
        started_at: &'a Instant,
    ) -> ChatErrorContext<'a> {
        ChatErrorContext {
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
        request: ChatCompletionRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        let prepared = self.prepare_chat_relay(&ctx, &request).await?;

        if request.stream {
            return self.relay_stream(&ctx, &request, prepared).await;
        }

        self.relay_non_stream(&ctx, &request, prepared).await
    }

    async fn relay_non_stream(
        &self,
        ctx: &RelayChatContext,
        request: &ChatCompletionRequest,
        prepared: PreparedChatRelay,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedChatRelay {
            request_id,
            started_at,
            tracked_request,
            mut tracked_execution,
            billing,
            mut log_context,
            target,
            provider,
            request_builder,
        } = prepared;
        let retry_request_builder = request_builder.try_clone();
        let mut first_request_builder = Some(request_builder);
        let mut pending_retry_attempt = None;

        for attempt_no in 1..=RELAY_MAX_UPSTREAM_ATTEMPTS {
            let attempt_started_at = Instant::now();
            if attempt_no > 1 {
                tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
                    let upstream_body =
                        build_tracking_upstream_body(request, &target.upstream_model);
                    match self
                        .tracking
                        .create_chat_execution(
                            tracked_request.id,
                            &request_id,
                            attempt_no,
                            request,
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            upstream_body,
                        )
                        .await
                    {
                        Ok(model) => Some(model),
                        Err(error) => {
                            tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry request_execution tracking row");
                            None
                        }
                    }
                } else {
                    None
                };

                log_context = build_chat_log_context(
                    ctx,
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
            }

            let error_ctx = self.error_context(
                tracked_request.as_ref(),
                tracked_execution.as_ref(),
                Some(&log_context),
                Some(&billing),
                Some(&target.upstream_model),
                &started_at,
            );
            let request_builder = if attempt_no == 1 {
                first_request_builder
                    .take()
                    .expect("first chat request builder must exist")
            } else {
                let Some(retry_template) = retry_request_builder.as_ref() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "chat request builder is not cloneable for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                let Some(cloned) = retry_template.try_clone() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "failed to clone chat request builder for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                cloned
            };

            let upstream_response = match self
                .send_upstream_chat(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_execution.as_ref(),
                        None,
                        &error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    if advance_relay_retry(
                        &self.tracking,
                        "chat",
                        &request_id,
                        attempt_no,
                        &mut pending_retry_attempt,
                        &error.error.error.message,
                        build_retry_attempt_payload(
                            tracked_execution.as_ref().map(|model| model.id),
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            error.status as i32,
                            "send_upstream",
                            None,
                        ),
                        true,
                    )
                    .await
                    {
                        continue;
                    }

                    return Err(error_ctx.finish(None, error).await);
                }
            };

            if let Some(error) = upstream_response.error {
                let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                self.try_finish_execution_failure(
                    &self.tracking,
                    tracked_execution.as_ref(),
                    upstream_response.upstream_request_id.as_deref(),
                    &error,
                    None,
                    attempt_duration_ms,
                )
                .await;

                if advance_relay_retry(
                    &self.tracking,
                    "chat",
                    &request_id,
                    attempt_no,
                    &mut pending_retry_attempt,
                    &error.error.error.message,
                    build_retry_attempt_payload(
                        tracked_execution.as_ref().map(|model| model.id),
                        target.channel.id,
                        target.account.id,
                        &target.upstream_model,
                        error.status as i32,
                        "upstream_status",
                        upstream_response.upstream_request_id.as_deref(),
                    ),
                    is_retryable_upstream_error(&error),
                )
                .await
                {
                    continue;
                }

                return Err(error_ctx
                    .finish(upstream_response.upstream_request_id.as_deref(), error)
                    .await);
            }

            let chat_response = match provider
                .parse_chat_response(upstream_response.body, &target.upstream_model)
            {
                Ok(response) => response,
                Err(error) => {
                    let openai_error = OpenAiErrorResponse::internal_with(
                        "failed to parse upstream chat response",
                        error,
                    );
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_execution.as_ref(),
                        upstream_response.upstream_request_id.as_deref(),
                        &openai_error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    advance_relay_retry(
                        &self.tracking,
                        "chat",
                        &request_id,
                        attempt_no,
                        &mut pending_retry_attempt,
                        &openai_error.error.error.message,
                        build_retry_attempt_payload(
                            tracked_execution.as_ref().map(|model| model.id),
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            openai_error.status as i32,
                            "parse_response",
                            upstream_response.upstream_request_id.as_deref(),
                        ),
                        false,
                    )
                    .await;

                    return Err(error_ctx
                        .finish(
                            upstream_response.upstream_request_id.as_deref(),
                            openai_error,
                        )
                        .await);
                }
            };

            let duration_ms = started_at.elapsed().as_millis() as i32;
            let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
            self.try_finish_request_success(
                &self.tracking,
                tracked_request.as_ref(),
                &target.upstream_model,
                upstream_response.status_code,
                &chat_response,
                duration_ms,
            )
            .await;
            self.try_finish_execution_success(
                &self.tracking,
                tracked_execution.as_ref(),
                upstream_response.upstream_request_id.as_deref(),
                upstream_response.status_code,
                &chat_response,
                attempt_duration_ms,
            )
            .await;
            if attempt_no > 1 {
                complete_relay_retry_success(
                    &self.tracking,
                    &request_id,
                    pending_retry_attempt.as_ref(),
                    build_retry_attempt_payload(
                        tracked_execution.as_ref().map(|model| model.id),
                        target.channel.id,
                        target.account.id,
                        &target.upstream_model,
                        upstream_response.status_code,
                        "retry_succeeded",
                        upstream_response.upstream_request_id.as_deref(),
                    ),
                )
                .await;
            }
            self.try_settle_chat_billing_success(&request_id, &billing, &chat_response.usage)
                .await;
            self.try_record_chat_success_log(
                ctx,
                &target,
                tracked_execution
                    .as_ref()
                    .map(|model| model.id)
                    .unwrap_or(0),
                &billing,
                &request.model,
                &chat_response,
                &request_id,
                upstream_response
                    .upstream_request_id
                    .as_deref()
                    .unwrap_or_default(),
                duration_ms,
            )
            .await;

            return Ok(Json::<ChatCompletionResponse>(chat_response).into_response());
        }

        Err(OpenAiErrorResponse::internal_with(
            "chat relay exhausted retry attempts",
            "no retry attempt produced a terminal result",
        ))
    }

    async fn send_upstream_chat(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamChatResponse, OpenAiErrorResponse> {
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
            Ok(UpstreamChatResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: None,
            })
        } else {
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamChatResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: Some(provider_error_to_openai_response(status.as_u16(), &info)),
            })
        }
    }

    async fn prepare_chat_relay(
        &self,
        ctx: &RelayChatContext,
        request: &ChatCompletionRequest,
    ) -> Result<PreparedChatRelay, OpenAiErrorResponse> {
        let PreparedRequestMeta {
            request_id,
            started_at,
        } = prepare_request_meta(&ctx.token_info, "chat", &request.model)?;

        let tracked_request = try_create_tracked_request(
            &request_id,
            self.tracking.create_chat_request(
                &request_id,
                &ctx.token_info,
                request,
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

        let target = match self.resolve_target(&ctx.token_info.group, request).await {
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
            .prepare_chat_billing(&ctx.token_info, request, &target)
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
            let upstream_body = build_tracking_upstream_body(request, &target.upstream_model);
            match self
                .tracking
                .create_chat_execution(
                    tracked_request.id,
                    &request_id,
                    1,
                    request,
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

        let log_context = build_chat_log_context(
            ctx,
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

        let provider = ProviderRegistry::chat(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("chat endpoint is disabled")
        })?;
        let error_ctx = self.error_context(
            tracked_request.as_ref(),
            tracked_execution.as_ref(),
            Some(&log_context),
            Some(&billing),
            Some(&target.upstream_model),
            &started_at,
        );

        let request_builder = match provider.build_chat_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            request,
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                return Err(error_ctx
                    .finish(
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream chat request",
                            error,
                        ),
                    )
                    .await);
            }
        };

        Ok(PreparedChatRelay {
            request_id,
            started_at,
            tracked_request,
            tracked_execution,
            billing,
            log_context,
            target,
            provider,
            request_builder,
        })
    }

    async fn prepare_chat_billing(
        &self,
        token_info: &TokenInfo,
        request: &ChatCompletionRequest,
        target: &ResolvedChatTarget,
    ) -> Result<ChatBillingContext, OpenAiErrorResponse> {
        let price = self
            .billing
            .resolve_effective_price(target.channel.id, &request.model, "chat")
            .await
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        let group_ratio = self
            .billing
            .get_group_ratio(&token_info.group)
            .await
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        let estimated_tokens = BillingEngine::estimate_prompt_tokens(&request.messages);
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

        Ok(ChatBillingContext {
            token_id: token_info.token_id,
            unlimited_quota: token_info.unlimited_quota,
            group_ratio,
            pre_consumed,
            estimated_prompt_tokens: estimated_tokens,
            price,
        })
    }

    async fn resolve_target(
        &self,
        channel_group: &str,
        request: &ChatCompletionRequest,
    ) -> ApiResult<ResolvedChatTarget> {
        resolve_relay_target(&self.db, channel_group, "chat", &request.model).await
    }

    async fn finish_with_error(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        tracked_execution: Option<&request_execution::Model>,
        log_context: Option<&ChatLogContext>,
        billing: Option<&ChatBillingContext>,
        upstream_model: Option<&str>,
        upstream_request_id: Option<&str>,
        openai_error: OpenAiErrorResponse,
        duration_ms: i32,
    ) -> OpenAiErrorResponse {
        self.try_refund_chat_billing("chat", billing).await;
        let error_body =
            serde_json::to_value(&openai_error.error).unwrap_or_else(|_| serde_json::json!({}));
        self.try_record_chat_failure_log(
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

    async fn try_settle_chat_billing_success(
        &self,
        request_id: &str,
        billing: &ChatBillingContext,
        usage: &summer_ai_core::types::common::Usage,
    ) {
        if let Err(error) = self
            .billing
            .post_consume(
                billing.token_id,
                billing.unlimited_quota,
                billing.pre_consumed,
                usage,
                &billing.price,
                billing.group_ratio,
            )
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to settle chat billing");
        }
    }

    async fn try_refund_chat_billing(
        &self,
        request_id: &str,
        billing: Option<&ChatBillingContext>,
    ) {
        let Some(billing) = billing else {
            return;
        };

        if let Err(error) = self
            .billing
            .refund(billing.token_id, billing.pre_consumed)
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to refund chat billing reservation");
        }
    }

    async fn try_record_chat_success_log(
        &self,
        ctx: &RelayChatContext,
        target: &ResolvedChatTarget,
        execution_id: i64,
        billing: &ChatBillingContext,
        requested_model: &str,
        response: &ChatCompletionResponse,
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
            endpoint: "/v1/chat/completions".into(),
            request_format: "openai/chat_completions".into(),
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
            tracing::warn!(request_id, error = %error, "failed to write chat usage log");
        }
    }

    async fn try_record_chat_failure_log(
        &self,
        log_context: Option<&ChatLogContext>,
        billing: Option<&ChatBillingContext>,
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
            endpoint: "/v1/chat/completions".into(),
            request_format: "openai/chat_completions".into(),
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
            tracing::warn!(request_id, error = %error, "failed to write chat failure log");
        }
    }

    async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &ChatCompletionResponse,
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
        response_body: &ChatCompletionResponse,
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

fn build_tracking_upstream_body(
    request: &ChatCompletionRequest,
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

fn build_chat_log_context(
    ctx: &RelayChatContext,
    channel_id: i64,
    channel_name: &str,
    account_id: i64,
    account_name: &str,
    execution_id: i64,
    requested_model: &str,
) -> ChatLogContext {
    ChatLogContext {
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
