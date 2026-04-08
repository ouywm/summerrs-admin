use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};

use summer_ai_model::entity::requests::retry_attempt::{self, RetryAttemptStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryAttemptQuery {
    pub domain_code: Option<String>,
    pub task_type: Option<String>,
    pub reference_id: Option<String>,
    pub request_id: Option<String>,
    pub attempt_no: Option<i32>,
    pub status: Option<RetryAttemptStatus>,
    pub next_retry_at_start: Option<DateTime<FixedOffset>>,
    pub next_retry_at_end: Option<DateTime<FixedOffset>>,
    pub create_time_start: Option<DateTime<FixedOffset>>,
    pub create_time_end: Option<DateTime<FixedOffset>>,
}

impl From<RetryAttemptQuery> for Condition {
    fn from(req: RetryAttemptQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(domain_code) = req.domain_code {
            condition = condition.add(retry_attempt::Column::DomainCode.eq(domain_code));
        }
        if let Some(task_type) = req.task_type {
            condition = condition.add(retry_attempt::Column::TaskType.eq(task_type));
        }
        if let Some(reference_id) = req.reference_id {
            condition = condition.add(retry_attempt::Column::ReferenceId.contains(&reference_id));
        }
        if let Some(request_id) = req.request_id {
            condition = condition.add(retry_attempt::Column::RequestId.contains(&request_id));
        }
        if let Some(attempt_no) = req.attempt_no {
            condition = condition.add(retry_attempt::Column::AttemptNo.eq(attempt_no));
        }
        if let Some(status) = req.status {
            condition = condition.add(retry_attempt::Column::Status.eq(status));
        }
        if let Some(next_retry_at_start) = req.next_retry_at_start {
            condition = condition.add(retry_attempt::Column::NextRetryAt.gte(next_retry_at_start));
        }
        if let Some(next_retry_at_end) = req.next_retry_at_end {
            condition = condition.add(retry_attempt::Column::NextRetryAt.lte(next_retry_at_end));
        }
        if let Some(create_time_start) = req.create_time_start {
            condition = condition.add(retry_attempt::Column::CreateTime.gte(create_time_start));
        }
        if let Some(create_time_end) = req.create_time_end {
            condition = condition.add(retry_attempt::Column::CreateTime.lte(create_time_end));
        }
        condition
    }
}
