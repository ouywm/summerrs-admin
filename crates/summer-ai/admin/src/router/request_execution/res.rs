use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::requests::request_execution::{self, RequestExecutionStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestExecutionListRes {
    pub id: i64,
    pub ai_request_id: i64,
    pub request_id: String,
    pub attempt_no: i32,
    pub channel_id: i64,
    pub account_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub upstream_request_id: String,
    pub response_status_code: i32,
    pub status: RequestExecutionStatus,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub started_at: DateTime<FixedOffset>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl RequestExecutionListRes {
    pub fn from_model(model: request_execution::Model) -> Self {
        Self {
            id: model.id,
            ai_request_id: model.ai_request_id,
            request_id: model.request_id,
            attempt_no: model.attempt_no,
            channel_id: model.channel_id,
            account_id: model.account_id,
            endpoint: model.endpoint,
            request_format: model.request_format,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            upstream_request_id: model.upstream_request_id,
            response_status_code: model.response_status_code,
            status: model.status,
            duration_ms: model.duration_ms,
            first_token_ms: model.first_token_ms,
            started_at: model.started_at,
            finished_at: model.finished_at,
            create_time: model.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestExecutionDetailRes {
    pub id: i64,
    pub ai_request_id: i64,
    pub request_id: String,
    pub attempt_no: i32,
    pub channel_id: i64,
    pub account_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub upstream_request_id: String,
    pub request_headers: serde_json::Value,
    pub request_body: serde_json::Value,
    pub response_body: Option<serde_json::Value>,
    pub response_status_code: i32,
    pub status: RequestExecutionStatus,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub started_at: DateTime<FixedOffset>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl RequestExecutionDetailRes {
    pub fn from_model(model: request_execution::Model) -> Self {
        Self {
            id: model.id,
            ai_request_id: model.ai_request_id,
            request_id: model.request_id,
            attempt_no: model.attempt_no,
            channel_id: model.channel_id,
            account_id: model.account_id,
            endpoint: model.endpoint,
            request_format: model.request_format,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            upstream_request_id: model.upstream_request_id,
            request_headers: model.request_headers,
            request_body: model.request_body,
            response_body: model.response_body,
            response_status_code: model.response_status_code,
            status: model.status,
            error_message: model.error_message,
            duration_ms: model.duration_ms,
            first_token_ms: model.first_token_ms,
            started_at: model.started_at,
            finished_at: model.finished_at,
            create_time: model.create_time,
        }
    }
}
