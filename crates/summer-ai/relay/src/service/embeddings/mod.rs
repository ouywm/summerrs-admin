use std::time::Instant;

use bytes::Bytes;
use summer::plugin::Service;
use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::provider::{EmbeddingProvider, ProviderKind, ProviderRegistry};
use summer_ai_core::types::embedding::{
    EmbeddingRequest, EmbeddingResponse, estimate_input_tokens,
};
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::response::{IntoResponse, Response};

use crate::service::chat::RelayChatContext;
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
use crate::service::tracking::{
    CreateTraceTracking, TrackingService, build_request_trace_failure_metadata,
    build_request_trace_success_metadata, execution_trace_span_failure_metadata,
    execution_trace_span_success_metadata, request_trace_failure_metadata,
    request_trace_success_metadata,
};

mod non_stream;
mod support;

use self::support::{
    EmbeddingsBillingContext, EmbeddingsErrorContext, EmbeddingsLogContext, PreparedEmbeddingsRelay,
};

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
        trace_id: Option<i64>,
        tracked_request: Option<&'a request::Model>,
        tracked_execution: Option<&'a request_execution::Model>,
        log_context: Option<&'a EmbeddingsLogContext>,
        billing: Option<&'a EmbeddingsBillingContext>,
        upstream_model: Option<&'a str>,
        started_at: &'a Instant,
    ) -> EmbeddingsErrorContext<'a> {
        EmbeddingsErrorContext {
            service: self,
            trace_id,
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
            trace_key,
            started_at,
        } = prepare_request_meta(&ctx.token_info, "embeddings", &request.model)?;
        let tracking = &self.tracking;

        let trace_id = match tracking
            .create_trace(CreateTraceTracking {
                trace_key: &trace_key,
                root_request_id: &request_id,
                user_id: ctx.token_info.user_id,
                metadata: serde_json::json!({
                    "endpoint": "/v1/embeddings",
                    "request_format": "openai/embeddings",
                    "requested_model": request.model,
                    "is_stream": false,
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
            tracking.create_embeddings_request(
                &request_id,
                trace_id.unwrap_or(0),
                &ctx.token_info,
                &request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            ),
        )
        .await;
        let base_error_ctx = self.error_context(
            trace_id,
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
                        trace_id,
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

        let provider = ProviderRegistry::embedding(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("embeddings endpoint is disabled")
        })?;
        let estimated_prompt_tokens = estimate_input_tokens(&request.input);
        let base_request_builder = {
            let error_ctx = self.error_context(
                trace_id,
                tracked_request.as_ref(),
                None,
                None,
                Some(&billing),
                Some(&target.upstream_model),
                &started_at,
            );
            match provider.build_embedding_request(
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
            }
        };

        self.relay_non_stream_with_retry(
            &ctx,
            &request,
            PreparedEmbeddingsRelay {
                request_id,
                trace_id,
                started_at,
                tracked_request,
                target,
                billing,
                provider,
                estimated_prompt_tokens,
                base_request_builder,
            },
        )
        .await
    }
}
