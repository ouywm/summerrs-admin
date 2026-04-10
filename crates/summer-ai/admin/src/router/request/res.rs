use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::requests::request::{self, RequestStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestListRes {
    pub id: i64,
    pub request_id: String,
    pub user_id: i64,
    pub token_id: i64,
    pub channel_group: String,
    pub source_type: String,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub is_stream: bool,
    pub response_status_code: i32,
    pub status: RequestStatus,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl RequestListRes {
    pub fn from_model(model: request::Model) -> Self {
        Self {
            id: model.id,
            request_id: model.request_id,
            user_id: model.user_id,
            token_id: model.token_id,
            channel_group: model.channel_group,
            source_type: model.source_type,
            endpoint: model.endpoint,
            request_format: model.request_format,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            is_stream: model.is_stream,
            response_status_code: model.response_status_code,
            status: model.status,
            duration_ms: model.duration_ms,
            first_token_ms: model.first_token_ms,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetailRes {
    pub id: i64,
    pub request_id: String,
    pub user_id: i64,
    pub token_id: i64,
    pub project_id: i64,
    pub conversation_id: i64,
    pub message_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub trace_id: i64,
    pub channel_group: String,
    pub source_type: String,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub is_stream: bool,
    pub client_ip: String,
    pub user_agent: String,
    pub request_headers: serde_json::Value,
    pub request_body: serde_json::Value,
    pub response_body: Option<serde_json::Value>,
    pub response_status_code: i32,
    pub status: RequestStatus,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl RequestDetailRes {
    pub fn from_model(model: request::Model) -> Self {
        Self {
            id: model.id,
            request_id: model.request_id,
            user_id: model.user_id,
            token_id: model.token_id,
            project_id: model.project_id,
            conversation_id: model.conversation_id,
            message_id: model.message_id,
            session_id: model.session_id,
            thread_id: model.thread_id,
            trace_id: model.trace_id,
            channel_group: model.channel_group,
            source_type: model.source_type,
            endpoint: model.endpoint,
            request_format: model.request_format,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            is_stream: model.is_stream,
            client_ip: model.client_ip,
            user_agent: model.user_agent,
            request_headers: model.request_headers,
            request_body: model.request_body,
            response_body: model.response_body,
            response_status_code: model.response_status_code,
            status: model.status,
            error_message: model.error_message,
            duration_ms: model.duration_ms,
            first_token_ms: model.first_token_ms,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}
