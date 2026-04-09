use super::*;

pub(super) struct PreparedEmbeddingsRelay {
    pub(super) request_id: String,
    pub(super) started_at: Instant,
    pub(super) tracked_request: Option<request::Model>,
    pub(super) target: ResolvedEmbeddingsTarget,
    pub(super) billing: EmbeddingsBillingContext,
    pub(super) provider: &'static dyn EmbeddingProvider,
    pub(super) estimated_prompt_tokens: i32,
    pub(super) base_request_builder: reqwest::RequestBuilder,
}

impl EmbeddingsRelayService {
    pub(super) async fn resolve_target(
        &self,
        channel_group: &str,
        request: &EmbeddingRequest,
    ) -> ApiResult<ResolvedEmbeddingsTarget> {
        resolve_relay_target(&self.db, channel_group, "embeddings", &request.model).await
    }

    pub(super) async fn finish_with_error(
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

    pub(super) async fn prepare_embeddings_billing(
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

    pub(super) async fn try_settle_embeddings_billing_success(
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

    pub(super) async fn try_refund_embeddings_billing(
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

    pub(super) async fn try_record_embeddings_success_log(
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

    pub(super) async fn try_record_embeddings_failure_log(
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

    pub(super) async fn try_finish_request_success(
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

    pub(super) async fn try_finish_request_failure(
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

    pub(super) async fn try_finish_execution_success(
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

    pub(super) async fn try_finish_execution_failure(
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

pub(super) type ResolvedEmbeddingsTarget = ResolvedRelayTarget;

pub(super) struct UpstreamEmbeddingsResponse {
    pub(super) status_code: i32,
    pub(super) upstream_request_id: Option<String>,
    pub(super) body: Bytes,
    pub(super) error: Option<OpenAiErrorResponse>,
}

pub(super) struct EmbeddingsLogContext {
    pub(super) token_info: TokenInfo,
    pub(super) channel_id: i64,
    pub(super) channel_name: String,
    pub(super) account_id: i64,
    pub(super) account_name: String,
    pub(super) execution_id: i64,
    pub(super) requested_model: String,
    pub(super) client_ip: String,
    pub(super) user_agent: String,
}

pub(super) struct EmbeddingsBillingContext {
    pub(super) token_id: i64,
    pub(super) unlimited_quota: bool,
    pub(super) group_ratio: f64,
    pub(super) pre_consumed: i64,
    pub(super) price: ResolvedModelPrice,
}

pub(super) struct EmbeddingsErrorContext<'a> {
    pub(super) service: &'a EmbeddingsRelayService,
    pub(super) tracked_request: Option<&'a request::Model>,
    pub(super) tracked_execution: Option<&'a request_execution::Model>,
    pub(super) log_context: Option<&'a EmbeddingsLogContext>,
    pub(super) billing: Option<&'a EmbeddingsBillingContext>,
    pub(super) upstream_model: Option<&'a str>,
    pub(super) started_at: &'a Instant,
}

impl<'a> EmbeddingsErrorContext<'a> {
    pub(super) async fn finish(
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

pub(super) fn build_tracking_upstream_body(
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

pub(super) fn build_embeddings_log_context(
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
    use super::build_tracking_upstream_body;
    use summer_ai_core::types::embedding::EmbeddingRequest;

    #[test]
    fn build_tracking_upstream_body_overrides_model_and_keeps_input() {
        let request = EmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: serde_json::json!(["hello", "world"]),
            encoding_format: None,
            dimensions: None,
            user: None,
            extra: serde_json::Map::new(),
        };

        let body = build_tracking_upstream_body(&request, "text-embedding-3-large");

        assert_eq!(body["model"], "text-embedding-3-large");
        assert_eq!(body["input"], serde_json::json!(["hello", "world"]));
    }
}
