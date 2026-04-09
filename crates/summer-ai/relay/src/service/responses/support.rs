use anyhow::Context;

use super::*;

impl ResponsesRelayService {
    pub(super) async fn prepare_responses_relay(
        &self,
        ctx: &RelayChatContext,
        request: &ResponsesRequest,
    ) -> Result<PreparedResponsesRelay, OpenAiErrorResponse> {
        let PreparedRequestMeta {
            request_id,
            trace_key,
            started_at,
        } = prepare_request_meta(&ctx.token_info, "responses", &request.model)?;
        let tracking = &self.tracking;

        let trace_id = match tracking
            .create_trace(CreateTraceTracking {
                trace_key: &trace_key,
                root_request_id: &request_id,
                user_id: ctx.token_info.user_id,
                metadata: serde_json::json!({
                    "endpoint": "/v1/responses",
                    "request_format": "openai/responses",
                    "requested_model": request.model,
                    "is_stream": request.stream,
                    "channel_group": ctx.token_info.group,
                    "client_ip": ctx.client_ip,
                    "user_agent": ctx.user_agent,
                }),
            })
            .await
        {
            Ok(model) => Some(model.id),
            Err(error) => {
                tracing::warn!(request_id, error = %error, "failed to create trace tracking row");
                None
            }
        };

        let tracked_request = try_create_tracked_request(
            &request_id,
            tracking.create_responses_request(
                &request_id,
                trace_id.unwrap_or(0),
                &ctx.token_info,
                request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            ),
        )
        .await;
        let base_error_ctx = self.error_context(
            trace_id,
            request.stream,
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
            .prepare_responses_billing(&ctx.token_info, request, &target)
            .await
        {
            Ok(billing) => billing,
            Err(error) => {
                return Err(self
                    .error_context(
                        trace_id,
                        request.stream,
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
            let tracked_execution = match tracking
                .create_responses_execution(
                    tracked_request.id,
                    &request_id,
                    1,
                    request,
                    target.channel.id,
                    target.account.id,
                    &target.upstream_model,
                    upstream_body.clone(),
                )
                .await
            {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::warn!(request_id, error = %error, "failed to create request_execution tracking row");
                    None
                }
            };

            if let Some(trace_id) = trace_id
                && let Err(error) = tracking
                    .create_execution_trace_span(
                        trace_id,
                        &request_id,
                        "responses",
                        1,
                        &request.model,
                        &target.upstream_model,
                        target.channel.id,
                        target.account.id,
                        upstream_body,
                    )
                    .await
            {
                tracing::warn!(request_id, error = %error, "failed to create trace span tracking row");
            }

            tracked_execution
        } else {
            None
        };

        let log_context = build_responses_log_context(
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
        let provider = ProviderRegistry::responses(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("responses endpoint is disabled")
        })?;
        let runtime_mode = provider.runtime_mode();

        let request_builder = match provider.build_responses_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            &serde_json::to_value(request).unwrap_or_else(|_| serde_json::json!({})),
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let error_ctx = self.error_context(
                    trace_id,
                    request.stream,
                    tracked_request.as_ref(),
                    tracked_execution.as_ref(),
                    Some(&log_context),
                    Some(&billing),
                    Some(&target.upstream_model),
                    &started_at,
                );
                return Err(error_ctx
                    .finish(
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream responses request",
                            error,
                        ),
                    )
                    .await);
            }
        };

        Ok(PreparedResponsesRelay {
            request_id,
            trace_id,
            started_at,
            tracked_request,
            tracked_execution,
            billing,
            log_context,
            target,
            runtime_mode,
            request_builder,
        })
    }

    pub(super) async fn resolve_target(
        &self,
        channel_group: &str,
        request: &ResponsesRequest,
    ) -> ApiResult<ResolvedResponsesTarget> {
        resolve_relay_target(&self.db, channel_group, "responses", &request.model).await
    }

    pub(super) async fn finish_with_error(
        &self,
        tracking: &TrackingService,
        trace_id: Option<i64>,
        is_stream: bool,
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
            trace_id,
            tracked_request
                .map(|tracked_request| tracked_request.request_id.as_str())
                .unwrap_or_default(),
            "/v1/responses",
            "openai/responses",
            log_context
                .map(|log_context| log_context.requested_model.as_str())
                .unwrap_or_default(),
            is_stream,
            tracked_request,
            upstream_model,
            &openai_error,
            Some(error_body.clone()),
            duration_ms,
        )
        .await;
        self.try_finish_execution_failure(
            tracking,
            tracked_request.map(|model| model.trace_id),
            tracked_execution,
            upstream_request_id,
            &openai_error,
            Some(error_body),
            duration_ms,
        )
        .await;
        openai_error
    }

    pub(super) async fn prepare_responses_billing(
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

    pub(super) async fn try_settle_responses_billing_success(
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

    pub(super) async fn try_refund_responses_billing(
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

    pub(super) async fn try_record_responses_success_log(
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

    pub(super) async fn try_record_responses_failure_log(
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

        let record = FailureLogRecord {
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

        if let Err(error) = self
            .log
            .record_failure(&log_context.token_info, record)
            .await
        {
            tracing::warn!(request_id, error = %error, "failed to write responses failure log");
        }
    }

    pub(super) async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        trace_id: Option<i64>,
        request_id: &str,
        requested_model: &str,
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

            if tracked_request.trace_id > 0
                && let Err(error) = tracking
                    .finish_trace_success(
                        tracked_request.trace_id,
                        request_trace_success_metadata(
                            tracked_request,
                            upstream_model,
                            response_status_code,
                            duration_ms,
                            0,
                        ),
                    )
                    .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update trace success tracking row");
            }
        } else if let Some(trace_id) = trace_id
            && let Err(error) = tracking
                .finish_trace_success(
                    trace_id,
                    build_request_trace_success_metadata(
                        request_id,
                        "/v1/responses",
                        "openai/responses",
                        requested_model,
                        upstream_model,
                        false,
                        response_status_code,
                        duration_ms,
                        0,
                    ),
                )
                .await
        {
            tracing::warn!(request_id, error = %error, "failed to update trace success tracking row without request tracking");
        }
    }

    pub(super) async fn try_finish_request_failure(
        &self,
        tracking: &TrackingService,
        trace_id: Option<i64>,
        request_id: &str,
        endpoint: &str,
        request_format: &str,
        requested_model: &str,
        is_stream: bool,
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

            if tracked_request.trace_id > 0
                && let Err(error) = tracking
                    .finish_trace_failure(
                        tracked_request.trace_id,
                        request_trace_failure_metadata(
                            tracked_request,
                            upstream_model,
                            openai_error.status as i32,
                            &openai_error.error.error.message,
                            duration_ms,
                            0,
                        ),
                    )
                    .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update trace failure tracking row");
            }
        } else if let Some(trace_id) = trace_id
            && let Err(error) = tracking
                .finish_trace_failure(
                    trace_id,
                    build_request_trace_failure_metadata(
                        request_id,
                        endpoint,
                        request_format,
                        requested_model,
                        upstream_model,
                        is_stream,
                        openai_error.status as i32,
                        &openai_error.error.error.message,
                        duration_ms,
                        0,
                    ),
                )
                .await
        {
            tracing::warn!(request_id, error = %error, "failed to update trace failure tracking row without request tracking");
        }
    }

    pub(super) async fn try_finish_execution_success(
        &self,
        tracking: &TrackingService,
        trace_id: Option<i64>,
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

            if trace_id.unwrap_or(0) > 0
                && let Err(error) = tracking
                    .finish_execution_trace_span_success(
                        trace_id.unwrap_or_default(),
                        tracked_execution.attempt_no,
                        serde_json::to_value(response_body)
                            .unwrap_or_else(|_| serde_json::json!({})),
                        execution_trace_span_success_metadata(
                            tracked_execution,
                            upstream_request_id,
                            response_status_code,
                            duration_ms,
                            0,
                        ),
                    )
                    .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update trace span success tracking row");
            }
        }
    }

    pub(super) async fn try_finish_execution_failure(
        &self,
        tracking: &TrackingService,
        trace_id: Option<i64>,
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
                    response_body.clone(),
                    duration_ms,
                )
                .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update request_execution failure tracking row");
            }

            if trace_id.unwrap_or(0) > 0
                && let Err(error) = tracking
                    .finish_execution_trace_span_failure(
                        trace_id.unwrap_or_default(),
                        tracked_execution.attempt_no,
                        &openai_error.error.error.message,
                        response_body.unwrap_or_else(|| serde_json::json!({})),
                        execution_trace_span_failure_metadata(
                            tracked_execution,
                            upstream_request_id,
                            openai_error.status as i32,
                            &openai_error.error.error.message,
                            duration_ms,
                            0,
                        ),
                    )
                    .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update trace span failure tracking row");
            }
        }
    }

    pub(super) fn parse_responses_response(
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
}

pub(super) fn bridge_chat_response_to_responses_response(
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

pub(super) type ResolvedResponsesTarget = ResolvedRelayTarget;

pub(super) struct UpstreamResponsesResponse {
    pub(super) status_code: i32,
    pub(super) upstream_request_id: Option<String>,
    pub(super) body: Bytes,
    pub(super) error: Option<OpenAiErrorResponse>,
}

pub(super) enum UpstreamResponsesStreamResponse {
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
pub(crate) struct ResponsesBillingContext {
    pub(crate) token_id: i64,
    pub(crate) unlimited_quota: bool,
    pub(crate) group_ratio: f64,
    pub(crate) pre_consumed: i64,
    pub(crate) price: ResolvedModelPrice,
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

pub(super) struct PreparedResponsesRelay {
    pub(super) request_id: String,
    pub(super) trace_id: Option<i64>,
    pub(super) started_at: Instant,
    pub(super) tracked_request: Option<request::Model>,
    pub(super) tracked_execution: Option<request_execution::Model>,
    pub(super) billing: ResponsesBillingContext,
    pub(super) log_context: ResponsesLogContext,
    pub(super) target: ResolvedResponsesTarget,
    pub(super) runtime_mode: ResponsesRuntimeMode,
    pub(super) request_builder: reqwest::RequestBuilder,
}

pub(super) struct ResponsesErrorContext<'a> {
    pub(super) service: &'a ResponsesRelayService,
    pub(super) trace_id: Option<i64>,
    pub(super) is_stream: bool,
    pub(super) tracked_request: Option<&'a request::Model>,
    pub(super) tracked_execution: Option<&'a request_execution::Model>,
    pub(super) log_context: Option<&'a ResponsesLogContext>,
    pub(super) billing: Option<&'a ResponsesBillingContext>,
    pub(super) upstream_model: Option<&'a str>,
    pub(super) started_at: &'a Instant,
}

impl<'a> ResponsesErrorContext<'a> {
    pub(super) async fn finish(
        &self,
        upstream_request_id: Option<&str>,
        error: OpenAiErrorResponse,
    ) -> OpenAiErrorResponse {
        self.service
            .finish_with_error(
                &self.service.tracking,
                self.trace_id,
                self.is_stream,
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

pub(super) fn build_tracking_upstream_body(
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

pub(super) fn build_responses_log_context(
    ctx: &RelayChatContext,
    channel_id: i64,
    channel_name: &str,
    account_id: i64,
    account_name: &str,
    execution_id: i64,
    requested_model: &str,
) -> ResponsesLogContext {
    ResponsesLogContext {
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
