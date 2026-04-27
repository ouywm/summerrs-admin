use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::requests::log::{self, LogStatus, LogType};
use crate::entity::requests::request::{self, RequestStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogVo {
    pub id: i64,
    pub user_id: i64,
    pub token_id: i64,
    pub token_name: String,
    pub project_id: i64,
    pub channel_id: i64,
    pub channel_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub execution_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cached_tokens: i32,
    pub reasoning_tokens: i32,
    pub quota: i64,
    pub cost_total: f64,
    pub price_reference: String,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub request_id: String,
    pub upstream_request_id: String,
    pub status_code: i32,
    pub client_ip: String,
    pub content: String,
    pub log_type: LogType,
    pub status: LogStatus,
    pub request_status: Option<RequestStatus>,
    pub create_time: DateTimeWithTimeZone,
}

impl RequestLogVo {
    pub fn from_log(m: log::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            token_id: m.token_id,
            token_name: m.token_name,
            project_id: m.project_id,
            channel_id: m.channel_id,
            channel_name: m.channel_name,
            account_id: m.account_id,
            account_name: m.account_name,
            execution_id: m.execution_id,
            endpoint: m.endpoint,
            request_format: m.request_format,
            requested_model: m.requested_model,
            upstream_model: m.upstream_model,
            model_name: m.model_name,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            total_tokens: m.total_tokens,
            cached_tokens: m.cached_tokens,
            reasoning_tokens: m.reasoning_tokens,
            quota: m.quota,
            cost_total: ToPrimitive::to_f64(&m.cost_total).unwrap_or(0.0),
            price_reference: m.price_reference,
            elapsed_time: m.elapsed_time,
            first_token_time: m.first_token_time,
            is_stream: m.is_stream,
            request_id: m.request_id,
            upstream_request_id: m.upstream_request_id,
            status_code: m.status_code,
            client_ip: m.client_ip,
            content: m.content,
            log_type: m.log_type,
            status: m.status,
            request_status: None,
            create_time: m.create_time,
        }
    }

    pub fn from_log_and_request(m: log::Model, request: Option<request::Model>) -> Self {
        let mut vo = Self::from_log(m);
        vo.request_status = request.map(|r| r.status);
        vo
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetailVo {
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
    pub user_agent: String,
    pub request_headers: serde_json::Value,
    pub request_body: serde_json::Value,
    pub response_body: Option<serde_json::Value>,
    pub response_status_code: i32,
    pub status: RequestStatus,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}

impl RequestDetailVo {
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
            user_agent: m.user_agent,
            request_headers: m.request_headers,
            request_body: m.request_body,
            response_body: m.response_body,
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
