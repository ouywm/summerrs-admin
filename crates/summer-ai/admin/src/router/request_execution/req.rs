use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};

use summer_ai_model::entity::requests::request_execution::{self, RequestExecutionStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestExecutionQuery {
    pub ai_request_id: Option<i64>,
    pub request_id: Option<String>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub endpoint: Option<String>,
    pub request_format: Option<String>,
    pub requested_model: Option<String>,
    pub upstream_model: Option<String>,
    pub status: Option<RequestExecutionStatus>,
    pub response_status_code: Option<i32>,
    pub started_at_start: Option<DateTime<FixedOffset>>,
    pub started_at_end: Option<DateTime<FixedOffset>>,
}

impl From<RequestExecutionQuery> for Condition {
    fn from(req: RequestExecutionQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(ai_request_id) = req.ai_request_id {
            condition = condition.add(request_execution::Column::AiRequestId.eq(ai_request_id));
        }
        if let Some(request_id) = req.request_id {
            condition = condition.add(request_execution::Column::RequestId.contains(&request_id));
        }
        if let Some(channel_id) = req.channel_id {
            condition = condition.add(request_execution::Column::ChannelId.eq(channel_id));
        }
        if let Some(account_id) = req.account_id {
            condition = condition.add(request_execution::Column::AccountId.eq(account_id));
        }
        if let Some(endpoint) = req.endpoint {
            condition = condition.add(request_execution::Column::Endpoint.eq(endpoint));
        }
        if let Some(request_format) = req.request_format {
            condition = condition.add(request_execution::Column::RequestFormat.eq(request_format));
        }
        if let Some(requested_model) = req.requested_model {
            condition =
                condition.add(request_execution::Column::RequestedModel.contains(&requested_model));
        }
        if let Some(upstream_model) = req.upstream_model {
            condition =
                condition.add(request_execution::Column::UpstreamModel.contains(&upstream_model));
        }
        if let Some(status) = req.status {
            condition = condition.add(request_execution::Column::Status.eq(status));
        }
        if let Some(response_status_code) = req.response_status_code {
            condition = condition
                .add(request_execution::Column::ResponseStatusCode.eq(response_status_code));
        }
        if let Some(started_at_start) = req.started_at_start {
            condition = condition.add(request_execution::Column::StartedAt.gte(started_at_start));
        }
        if let Some(started_at_end) = req.started_at_end {
            condition = condition.add(request_execution::Column::StartedAt.lte(started_at_end));
        }
        condition
    }
}
