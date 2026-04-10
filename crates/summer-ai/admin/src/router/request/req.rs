use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};

use summer_ai_model::entity::requests::request::{self, RequestStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestQuery {
    pub request_id: Option<String>,
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub channel_group: Option<String>,
    pub source_type: Option<String>,
    pub endpoint: Option<String>,
    pub request_format: Option<String>,
    pub requested_model: Option<String>,
    pub upstream_model: Option<String>,
    pub status: Option<RequestStatus>,
    pub is_stream: Option<bool>,
    pub response_status_code: Option<i32>,
    pub create_time_start: Option<DateTime<FixedOffset>>,
    pub create_time_end: Option<DateTime<FixedOffset>>,
}

impl From<RequestQuery> for Condition {
    fn from(req: RequestQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(request_id) = req.request_id {
            condition = condition.add(request::Column::RequestId.contains(&request_id));
        }
        if let Some(user_id) = req.user_id {
            condition = condition.add(request::Column::UserId.eq(user_id));
        }
        if let Some(token_id) = req.token_id {
            condition = condition.add(request::Column::TokenId.eq(token_id));
        }
        if let Some(channel_group) = req.channel_group {
            condition = condition.add(request::Column::ChannelGroup.eq(channel_group));
        }
        if let Some(source_type) = req.source_type {
            condition = condition.add(request::Column::SourceType.eq(source_type));
        }
        if let Some(endpoint) = req.endpoint {
            condition = condition.add(request::Column::Endpoint.eq(endpoint));
        }
        if let Some(request_format) = req.request_format {
            condition = condition.add(request::Column::RequestFormat.eq(request_format));
        }
        if let Some(requested_model) = req.requested_model {
            condition = condition.add(request::Column::RequestedModel.contains(&requested_model));
        }
        if let Some(upstream_model) = req.upstream_model {
            condition = condition.add(request::Column::UpstreamModel.contains(&upstream_model));
        }
        if let Some(status) = req.status {
            condition = condition.add(request::Column::Status.eq(status));
        }
        if let Some(is_stream) = req.is_stream {
            condition = condition.add(request::Column::IsStream.eq(is_stream));
        }
        if let Some(response_status_code) = req.response_status_code {
            condition = condition.add(request::Column::ResponseStatusCode.eq(response_status_code));
        }
        if let Some(create_time_start) = req.create_time_start {
            condition = condition.add(request::Column::CreateTime.gte(create_time_start));
        }
        if let Some(create_time_end) = req.create_time_end {
            condition = condition.add(request::Column::CreateTime.lte(create_time_end));
        }
        condition
    }
}
