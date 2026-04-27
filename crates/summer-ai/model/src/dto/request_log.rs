use crate::entity::requests::log::{self, LogStatus, LogType};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogQueryDto {
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub project_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub status: Option<LogStatus>,
    pub log_type: Option<LogType>,
    pub endpoint: Option<String>,
    pub model_name: Option<String>,
    pub request_id: Option<String>,
    pub keyword: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<RequestLogQueryDto> for Condition {
    fn from(query: RequestLogQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.user_id {
            cond = cond.add(log::Column::UserId.eq(v));
        }
        if let Some(v) = query.token_id {
            cond = cond.add(log::Column::TokenId.eq(v));
        }
        if let Some(v) = query.project_id {
            cond = cond.add(log::Column::ProjectId.eq(v));
        }
        if let Some(v) = query.channel_id {
            cond = cond.add(log::Column::ChannelId.eq(v));
        }
        if let Some(v) = query.account_id {
            cond = cond.add(log::Column::AccountId.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(log::Column::Status.eq(v));
        }
        if let Some(v) = query.log_type {
            cond = cond.add(log::Column::LogType.eq(v));
        }
        if let Some(v) = query.endpoint {
            cond = cond.add(log::Column::Endpoint.eq(v));
        }
        if let Some(v) = query.model_name {
            cond = cond.add(log::Column::ModelName.eq(v));
        }
        if let Some(v) = query.request_id {
            cond = cond.add(log::Column::RequestId.eq(v));
        }
        if let Some(v) = query.start_time {
            cond = cond.add(log::Column::CreateTime.gte(v));
        }
        if let Some(v) = query.end_time {
            cond = cond.add(log::Column::CreateTime.lte(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(log::Column::TokenName.contains(&keyword))
                        .add(log::Column::ChannelName.contains(&keyword))
                        .add(log::Column::AccountName.contains(&keyword))
                        .add(log::Column::Content.contains(&keyword)),
                );
            }
        }
        cond
    }
}
