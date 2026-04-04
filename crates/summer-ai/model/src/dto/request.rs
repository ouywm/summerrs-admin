use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::entity::request::{self, RequestStatus};

/// 查询请求列表
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequestDto {
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub request_id: Option<String>,
    pub requested_model: Option<String>,
    pub status: Option<RequestStatus>,
    pub is_stream: Option<bool>,
    pub source_type: Option<String>,
    pub client_ip: Option<String>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
}

impl From<QueryRequestDto> for sea_orm::Condition {
    fn from(dto: QueryRequestDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            cond = cond.add(request::Column::UserId.eq(v));
        }
        if let Some(v) = dto.token_id {
            cond = cond.add(request::Column::TokenId.eq(v));
        }
        if let Some(v) = dto.request_id {
            cond = cond.add(request::Column::RequestId.eq(v));
        }
        if let Some(v) = dto.requested_model {
            cond = cond.add(request::Column::RequestedModel.contains(&v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(request::Column::Status.eq(v));
        }
        if let Some(v) = dto.is_stream {
            cond = cond.add(request::Column::IsStream.eq(v));
        }
        if let Some(v) = dto.source_type {
            cond = cond.add(request::Column::SourceType.eq(v));
        }
        if let Some(v) = dto.client_ip {
            cond = cond.add(request::Column::ClientIp.eq(v));
        }
        if let Some(v) = dto.start_time {
            cond = cond.add(request::Column::CreateTime.gte(v));
        }
        if let Some(v) = dto.end_time {
            cond = cond.add(request::Column::CreateTime.lte(v));
        }
        cond
    }
}
