use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::request::{self, RequestStatus};
use crate::entity::request_execution::{self, ExecutionStatus};

/// 请求主表 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestVo {
    pub id: i64,
    pub request_id: String,
    pub user_id: i64,
    pub token_id: i64,
    pub project_id: i64,
    pub channel_group: String,
    pub source_type: String,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub is_stream: bool,
    pub client_ip: String,
    pub response_status_code: i32,
    pub status: RequestStatus,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl RequestVo {
    pub fn from_model(m: request::Model) -> Self {
        Self {
            id: m.id,
            request_id: m.request_id,
            user_id: m.user_id,
            token_id: m.token_id,
            project_id: m.project_id,
            channel_group: m.channel_group,
            source_type: m.source_type,
            endpoint: m.endpoint,
            request_format: m.request_format,
            requested_model: m.requested_model,
            upstream_model: m.upstream_model,
            is_stream: m.is_stream,
            client_ip: m.client_ip,
            response_status_code: m.response_status_code,
            status: m.status,
            error_message: m.error_message,
            duration_ms: m.duration_ms,
            first_token_ms: m.first_token_ms,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

/// 请求详情 VO（含请求体/响应体）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetailVo {
    #[serde(flatten)]
    pub base: RequestVo,
    pub conversation_id: i64,
    pub message_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub trace_id: i64,
    pub user_agent: String,
    pub request_headers: serde_json::Value,
    pub request_body: serde_json::Value,
    pub response_body: Option<serde_json::Value>,
}

impl RequestDetailVo {
    pub fn from_model(m: request::Model) -> Self {
        let conversation_id = m.conversation_id;
        let message_id = m.message_id;
        let session_id = m.session_id;
        let thread_id = m.thread_id;
        let trace_id = m.trace_id;
        let user_agent = m.user_agent.clone();
        let request_headers = m.request_headers.clone();
        let request_body = m.request_body.clone();
        let response_body = m.response_body.clone();
        Self {
            base: RequestVo::from_model(m),
            conversation_id,
            message_id,
            session_id,
            thread_id,
            trace_id,
            user_agent,
            request_headers,
            request_body,
            response_body,
        }
    }
}

/// 执行尝试 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestExecutionVo {
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
    pub status: ExecutionStatus,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub started_at: DateTime<FixedOffset>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl RequestExecutionVo {
    pub fn from_model(m: request_execution::Model) -> Self {
        Self {
            id: m.id,
            ai_request_id: m.ai_request_id,
            request_id: m.request_id,
            attempt_no: m.attempt_no,
            channel_id: m.channel_id,
            account_id: m.account_id,
            endpoint: m.endpoint,
            request_format: m.request_format,
            requested_model: m.requested_model,
            upstream_model: m.upstream_model,
            upstream_request_id: m.upstream_request_id,
            response_status_code: m.response_status_code,
            status: m.status,
            error_message: m.error_message,
            duration_ms: m.duration_ms,
            first_token_ms: m.first_token_ms,
            started_at: m.started_at,
            finished_at: m.finished_at,
            create_time: m.create_time,
        }
    }
}

/// 请求详情（含执行尝试列表）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestWithExecutionsVo {
    #[serde(flatten)]
    pub request: RequestDetailVo,
    pub executions: Vec<RequestExecutionVo>,
}
