use super::support::{
    PreparedResponsesRelay, UpstreamResponsesResponse, build_responses_log_context,
    build_tracking_upstream_body,
};
use super::*;

impl ResponsesRelayService {
    pub(super) async fn relay_non_stream(
        &self,
        ctx: &RelayChatContext,
        request: &ResponsesRequest,
        prepared: PreparedResponsesRelay,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedResponsesRelay {
            request_id,
            started_at,
            tracked_request,
            mut tracked_execution,
            billing,
            mut log_context,
            target,
            runtime_mode,
            request_builder,
        } = prepared;
        let tracking = &self.tracking;
        let retry_request_builder = request_builder.try_clone();
        let mut first_request_builder = Some(request_builder);
        let mut pending_retry_attempt = None;

        for attempt_no in 1..=RELAY_MAX_UPSTREAM_ATTEMPTS {
            let attempt_started_at = Instant::now();
            if attempt_no > 1 {
                tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
                    let upstream_body =
                        build_tracking_upstream_body(request, &target.upstream_model);
                    match tracking
                        .create_responses_execution(
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

                log_context = build_responses_log_context(
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
                    .expect("first responses request builder must exist")
            } else {
                let Some(retry_template) = retry_request_builder.as_ref() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "responses request builder is not cloneable for retry",
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
                                "failed to clone responses request builder for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                cloned
            };

            let upstream_response = match self
                .send_upstream_responses(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        tracking,
                        tracked_execution.as_ref(),
                        None,
                        &error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    if advance_relay_retry(
                        tracking,
                        "responses",
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
                    tracking,
                    tracked_execution.as_ref(),
                    upstream_response.upstream_request_id.as_deref(),
                    &error,
                    None,
                    attempt_duration_ms,
                )
                .await;

                let retryable = is_retryable_upstream_error(&error);
                if advance_relay_retry(
                    tracking,
                    "responses",
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

            let responses_response = match self.parse_responses_response(
                runtime_mode,
                target.provider_kind,
                upstream_response.body,
                &target.upstream_model,
            ) {
                Ok(response) => response,
                Err(error) => {
                    let openai_error = OpenAiErrorResponse::internal_with(
                        "failed to parse upstream responses response",
                        error,
                    );
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        tracking,
                        tracked_execution.as_ref(),
                        upstream_response.upstream_request_id.as_deref(),
                        &openai_error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    advance_relay_retry(
                        tracking,
                        "responses",
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
                attempt_duration_ms,
            )
            .await;
            if attempt_no > 1 {
                complete_relay_retry_success(
                    tracking,
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
            self.try_settle_responses_billing_success(&request_id, &billing, &responses_response)
                .await;
            self.try_record_responses_success_log(
                ctx,
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

            return Ok(Json::<ResponsesResponse>(responses_response).into_response());
        }

        Err(OpenAiErrorResponse::internal_with(
            "responses relay exhausted retry attempts",
            "no retry attempt produced a terminal result",
        ))
    }

    pub(super) async fn send_upstream_responses(
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
}
