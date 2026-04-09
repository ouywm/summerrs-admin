use super::stream::{
    BridgeResponsesSseStreamArgs, NativeResponsesSseStreamArgs, ResponsesStreamCommonArgs,
    TrackedResponsesSseStream,
};
use super::support::{
    PreparedResponsesRelay, UpstreamResponsesStreamResponse, build_responses_log_context,
    build_tracking_upstream_body,
};
use super::*;

impl ResponsesRelayService {
    pub(super) async fn relay_stream(
        &self,
        ctx: &RelayChatContext,
        request: &ResponsesRequest,
        prepared: PreparedResponsesRelay,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedResponsesRelay {
            request_id,
            trace_id,
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
                    let tracked_execution = match tracking
                        .create_responses_execution(
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
                            tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry request_execution tracking row");
                            None
                        }
                    };

                    if tracked_request.trace_id > 0
                        && let Err(error) = tracking
                            .create_execution_trace_span(
                                tracked_request.trace_id,
                                &request_id,
                                "responses",
                                attempt_no,
                                &request.model,
                                &target.upstream_model,
                                target.channel.id,
                                target.account.id,
                                upstream_body,
                            )
                            .await
                    {
                        tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry trace span tracking row");
                    }

                    tracked_execution
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
                trace_id,
                request.stream,
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
                    .expect("first responses stream request builder must exist")
            } else {
                let Some(retry_template) = retry_request_builder.as_ref() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "responses stream request builder is not cloneable for retry",
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
                                "failed to clone responses stream request builder for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                cloned
            };

            let upstream_response = match self
                .send_upstream_responses_stream(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
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
                            "send_stream_upstream",
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

            let (response_status_code, upstream_request_id, response) = match upstream_response {
                UpstreamResponsesStreamResponse::Success {
                    status_code,
                    upstream_request_id,
                    response,
                } => (status_code, upstream_request_id, response),
                UpstreamResponsesStreamResponse::Failure {
                    upstream_request_id,
                    error,
                } => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        upstream_request_id.as_deref(),
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
                            "stream_upstream_status",
                            upstream_request_id.as_deref(),
                        ),
                        retryable,
                    )
                    .await
                    {
                        continue;
                    }

                    return Err(error_ctx
                        .finish(upstream_request_id.as_deref(), error)
                        .await);
                }
            };

            let common = ResponsesStreamCommonArgs {
                task_tracker: self.stream_task_tracker.clone(),
                tracking: Some(self.tracking.clone()),
                billing: Some(self.billing.clone()),
                billing_context: Some(billing.clone()),
                log: Some(self.log.clone()),
                log_context: Some(log_context.clone()),
                trace_id,
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
                    let chunk_stream =
                        match chat_provider.parse_chat_stream(response, &target.upstream_model) {
                            Ok(stream) => stream,
                            Err(error) => {
                                let openai_error = OpenAiErrorResponse::internal_with(
                                    "failed to parse bridged responses stream",
                                    error,
                                );
                                let attempt_duration_ms =
                                    attempt_started_at.elapsed().as_millis() as i32;
                                self.try_finish_execution_failure(
                                    tracking,
                                    tracked_request.as_ref().map(|model| model.trace_id),
                                    tracked_execution.as_ref(),
                                    upstream_request_id.as_deref(),
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
                                        "parse_stream",
                                        upstream_request_id.as_deref(),
                                    ),
                                    false,
                                )
                                .await;

                                return Err(error_ctx
                                    .finish(upstream_request_id.as_deref(), openai_error)
                                    .await);
                            }
                        };

                    TrackedResponsesSseStream::bridge(BridgeResponsesSseStreamArgs {
                        inner: chunk_stream,
                        common,
                    })
                }
            };

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
                        response_status_code,
                        "retry_succeeded",
                        upstream_request_id.as_deref(),
                    ),
                )
                .await;
            }

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

        Err(OpenAiErrorResponse::internal_with(
            "responses stream relay exhausted retry attempts",
            "no retry attempt produced a terminal result",
        ))
    }

    pub(super) async fn send_upstream_responses_stream(
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
}
