//! AI 用户额度实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 用户额度状态（1=正常, 2=禁用, 3=冻结）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum UserQuotaStatus {
    /// 正常
    #[sea_orm(num_value = 1)]
    Normal = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 冻结
    #[sea_orm(num_value = 3)]
    Frozen = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "user_quota")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户 ID
    pub user_id: i64,
    /// 渠道分组
    pub channel_group: String,
    /// 额度状态
    pub status: UserQuotaStatus,
    /// 总额度
    pub quota: i64,
    /// 已用额度
    pub used_quota: i64,
    /// 请求次数
    pub request_count: i64,
    /// 每日额度限制
    pub daily_quota_limit: i64,
    /// 每月额度限制
    pub monthly_quota_limit: i64,
    /// 每日已用额度
    pub daily_used_quota: i64,
    /// 每月已用额度
    pub monthly_used_quota: i64,
    /// 每日窗口开始时间
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    /// 每月窗口开始时间
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    /// 最后请求时间
    pub last_request_time: Option<DateTimeWithTimeZone>,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
