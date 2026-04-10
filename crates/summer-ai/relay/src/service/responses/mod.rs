use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
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
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::body::Body;
use summer_web::axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use summer_web::axum::http::{HeaderValue, StatusCode};
use summer_web::axum::response::{IntoResponse, Response};

use self::support::ResponsesErrorContext;
use crate::plugin::RelayStreamTaskTracker;
use crate::service::chat::RelayChatContext;
use crate::service::log::{FailureLogRecord, LogService, UsageLogRecord};
use crate::service::shared::relay::{
    RELAY_MAX_UPSTREAM_ATTEMPTS, ResolvedRelayTarget, advance_relay_retry,
    build_retry_attempt_payload, complete_relay_retry_success, extract_upstream_request_id,
    is_retryable_upstream_error, provider_error_to_openai_response, resolve_relay_target,
    stream_error_message, stream_error_status_code,
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
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;

mod non_stream;
mod stream;
mod stream_relay;
mod support;

use self::support::{ResponsesBillingContext, ResponsesLogContext};

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
        trace_id: Option<i64>,
        is_stream: bool,
        tracked_request: Option<&'a request::Model>,
        tracked_execution: Option<&'a request_execution::Model>,
        log_context: Option<&'a ResponsesLogContext>,
        billing: Option<&'a ResponsesBillingContext>,
        upstream_model: Option<&'a str>,
        started_at: &'a Instant,
    ) -> ResponsesErrorContext<'a> {
        ResponsesErrorContext {
            service: self,
            trace_id,
            is_stream,
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
        let prepared = self.prepare_responses_relay(&ctx, &request).await?;

        if request.stream {
            return self.relay_stream(&ctx, &request, prepared).await;
        }

        self.relay_non_stream(&ctx, &request, prepared).await
    }
}
