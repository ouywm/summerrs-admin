use super::support::{
    PreparedEmbeddingsRelay, UpstreamEmbeddingsResponse, build_embeddings_log_context,
    build_tracking_upstream_body,
};
use super::*;

impl EmbeddingsRelayService {
    pub(super) async fn relay_non_stream_with_retry(
        &self,
        ctx: &RelayChatContext,
        request: &EmbeddingRequest,
        prepared: PreparedEmbeddingsRelay,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedEmbeddingsRelay {
            request_id,
            trace_id,
            started_at,
            tracked_request,
            target,
            billing,
            provider,
            estimated_prompt_tokens,
            base_request_builder,
        } = prepared;

        let retry_request_builder = base_request_builder.try_clone();
        let mut first_request_builder = Some(base_request_builder);
        let mut pending_retry_attempt = None;

        for attempt_no in 1..=RELAY_MAX_UPSTREAM_ATTEMPTS {
            let attempt_started_at = Instant::now();
            let tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
                let upstream_body = build_tracking_upstream_body(request, &target.upstream_model);
                let tracked_execution = match self
                    .tracking
                    .create_embeddings_execution(
                        tracked_request.id,
                        &request_id,
                        attempt_no,
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
                        tracing::warn!(request_id, error = %error, attempt_no, "failed to create request_execution tracking row");
                        None
                    }
                };

                if tracked_request.trace_id > 0
                    && let Err(error) = self
                        .tracking
                        .create_execution_trace_span(
                            tracked_request.trace_id,
                            &request_id,
                            "embeddings",
                            attempt_no,
                            &request.model,
                            &target.upstream_model,
                            target.channel.id,
                            target.account.id,
                            upstream_body,
                        )
                        .await
                {
                    tracing::warn!(request_id, error = %error, attempt_no, "failed to create trace span tracking row");
                }

                tracked_execution
            } else {
                None
            };
            let log_context = build_embeddings_log_context(
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
            let error_ctx = self.error_context(
                trace_id,
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
                    .expect("first embeddings request builder must exist")
            } else {
                let Some(retry_template) = retry_request_builder.as_ref() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "embeddings request builder is not cloneable for retry",
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
                                "failed to clone embeddings request builder for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                cloned
            };

            let upstream_response = match self
                .send_upstream_embeddings(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        None,
                        &error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    if advance_relay_retry(
                        &self.tracking,
                        "embeddings",
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
                    tracked_request.as_ref().map(|model| model.trace_id),
                    tracked_execution.as_ref(),
                    upstream_response.upstream_request_id.as_deref(),
                    &error,
                    None,
                    attempt_duration_ms,
                )
                .await;

                let retryable = is_retryable_upstream_error(&error);
                if advance_relay_retry(
                    &self.tracking,
                    "embeddings",
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
                    retryable,
                )
                .await
                {
                    continue;
                }

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
                    let openai_error = OpenAiErrorResponse::internal_with(
                        "failed to parse upstream embeddings response",
                        error,
                    );
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        upstream_response.upstream_request_id.as_deref(),
                        &openai_error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    advance_relay_retry(
                        &self.tracking,
                        "embeddings",
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
                trace_id,
                &request_id,
                &request.model,
                tracked_request.as_ref(),
                &target.upstream_model,
                upstream_response.status_code,
                &embedding_response,
                duration_ms,
            )
            .await;
            self.try_finish_execution_success(
                &self.tracking,
                tracked_request.as_ref().map(|model| model.trace_id),
                tracked_execution.as_ref(),
                upstream_response.upstream_request_id.as_deref(),
                upstream_response.status_code,
                &embedding_response,
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
            self.try_settle_embeddings_billing_success(&request_id, &billing, &embedding_response)
                .await;
            self.try_record_embeddings_success_log(
                ctx,
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

            return Ok(Json::<EmbeddingResponse>(embedding_response).into_response());
        }

        Err(OpenAiErrorResponse::internal_with(
            "embeddings relay exhausted retry attempts",
            "no retry attempt produced a terminal result",
        ))
    }

    pub(super) async fn send_upstream_embeddings(
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
}
