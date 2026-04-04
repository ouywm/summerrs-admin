use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;

use crate::relay::channel_router::SelectedChannel;
use crate::service::log::{AiFailureLogRecord, LogService};
use crate::service::request::{ExecutionStatusUpdate, RequestService, RequestStatusUpdate};
use crate::service::token::TokenInfo;

#[derive(Debug, Clone, Copy, Default)]
pub struct RequestTrackingIds {
    pub request_id: Option<i64>,
    pub execution_id: Option<i64>,
}

pub struct FailureTrackingUpdate {
    pub status_code: i32,
    pub message: String,
    pub elapsed_ms: i64,
    pub upstream_model: Option<String>,
    pub upstream_request_id: Option<String>,
    pub response_body: Option<serde_json::Value>,
}

pub fn map_adapter_build_error(context: &str, error: anyhow::Error) -> OpenAiErrorResponse {
    let message = error.to_string();
    if message.contains("is not supported") {
        return OpenAiErrorResponse::unsupported_endpoint(message);
    }
    OpenAiErrorResponse::internal_with(context, error)
}

#[allow(clippy::too_many_arguments)]
pub fn record_terminal_failure(
    log_svc: &LogService,
    token_info: &TokenInfo,
    channel: &SelectedChannel,
    endpoint: &str,
    request_format: &str,
    requested_model: &str,
    upstream_model: &str,
    model_name: &str,
    request_id: &str,
    upstream_request_id: &str,
    elapsed_ms: i64,
    is_stream: bool,
    client_ip: &str,
    user_agent: &str,
    status_code: i32,
    message: impl Into<String>,
) {
    log_svc.record_failure_async(
        token_info,
        channel,
        AiFailureLogRecord {
            endpoint: endpoint.to_string(),
            request_format: request_format.to_string(),
            request_id: request_id.to_string(),
            upstream_request_id: upstream_request_id.to_string(),
            requested_model: requested_model.to_string(),
            upstream_model: upstream_model.to_string(),
            model_name: model_name.to_string(),
            elapsed_time: elapsed_ms as i32,
            is_stream,
            client_ip: client_ip.to_string(),
            user_agent: user_agent.to_string(),
            status_code,
            content: message.into(),
        },
    );
}

pub async fn update_request_failure_tracking(
    request_svc: &RequestService,
    tracking: RequestTrackingIds,
    failure: FailureTrackingUpdate,
) {
    request_svc
        .try_update_execution_status(
            tracking.execution_id,
            ExecutionStatusUpdate {
                status: ExecutionStatus::Failed,
                error_message: Some(failure.message.clone()),
                duration_ms: Some(failure.elapsed_ms as i32),
                first_token_ms: Some(0),
                response_status_code: Some(failure.status_code),
                response_body: failure.response_body.clone(),
                upstream_request_id: failure.upstream_request_id,
            },
        )
        .await;
    request_svc
        .try_update_request_status(
            tracking.request_id,
            RequestStatusUpdate {
                status: RequestStatus::Failed,
                error_message: Some(failure.message),
                duration_ms: Some(failure.elapsed_ms as i32),
                first_token_ms: Some(0),
                response_status_code: Some(failure.status_code),
                response_body: failure.response_body,
                upstream_model: failure.upstream_model,
            },
        )
        .await;
}

pub async fn update_request_success_tracking(
    request_svc: &RequestService,
    tracking: RequestTrackingIds,
    elapsed_ms: i64,
    first_token_ms: i32,
    upstream_model: String,
    upstream_request_id: String,
    response_body: Option<serde_json::Value>,
) {
    request_svc
        .try_update_execution_status(
            tracking.execution_id,
            ExecutionStatusUpdate {
                status: ExecutionStatus::Success,
                error_message: None,
                duration_ms: Some(elapsed_ms as i32),
                first_token_ms: Some(first_token_ms),
                response_status_code: Some(200),
                response_body: response_body.clone(),
                upstream_request_id: Some(upstream_request_id),
            },
        )
        .await;
    request_svc
        .try_update_request_status(
            tracking.request_id,
            RequestStatusUpdate {
                status: RequestStatus::Success,
                error_message: None,
                duration_ms: Some(elapsed_ms as i32),
                first_token_ms: Some(first_token_ms),
                response_status_code: Some(200),
                response_body,
                upstream_model: Some(upstream_model),
            },
        )
        .await;
}
