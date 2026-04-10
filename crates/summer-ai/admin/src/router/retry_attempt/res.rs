use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::requests::retry_attempt::{self, RetryAttemptStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryAttemptListRes {
    pub id: i64,
    pub domain_code: String,
    pub task_type: String,
    pub reference_id: String,
    pub request_id: String,
    pub attempt_no: i32,
    pub status: RetryAttemptStatus,
    pub backoff_seconds: i32,
    pub next_retry_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl RetryAttemptListRes {
    pub fn from_model(model: retry_attempt::Model) -> Self {
        Self {
            id: model.id,
            domain_code: model.domain_code,
            task_type: model.task_type,
            reference_id: model.reference_id,
            request_id: model.request_id,
            attempt_no: model.attempt_no,
            status: model.status,
            backoff_seconds: model.backoff_seconds,
            next_retry_at: model.next_retry_at,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryAttemptDetailRes {
    pub id: i64,
    pub domain_code: String,
    pub task_type: String,
    pub reference_id: String,
    pub request_id: String,
    pub attempt_no: i32,
    pub status: RetryAttemptStatus,
    pub backoff_seconds: i32,
    pub error_message: String,
    pub payload: serde_json::Value,
    pub next_retry_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl RetryAttemptDetailRes {
    pub fn from_model(model: retry_attempt::Model) -> Self {
        Self {
            id: model.id,
            domain_code: model.domain_code,
            task_type: model.task_type,
            reference_id: model.reference_id,
            request_id: model.request_id,
            attempt_no: model.attempt_no,
            status: model.status,
            backoff_seconds: model.backoff_seconds,
            error_message: model.error_message,
            payload: model.payload,
            next_retry_at: model.next_retry_at,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}
