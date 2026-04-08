//! AI 用户配额表（用户在 AI 网关中的额度与预算窗口）
//! 对应 sql/ai/user_quota.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=正常 2=禁用 3=冻结
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
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "user_quota")]
pub struct Model {
    /// 配额ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID（关联 sys."user".id，唯一）
    pub user_id: i64,
    /// 所属分组（决定可用渠道和计费倍率）
    pub channel_group: String,
    /// 状态：1=正常 2=禁用 3=冻结
    pub status: UserQuotaStatus,
    /// 总配额（累计授予额度）
    pub quota: i64,
    /// 累计已消耗配额
    pub used_quota: i64,
    /// 累计请求次数
    pub request_count: i64,
    /// 日额度上限（0=不限制）
    pub daily_quota_limit: i64,
    /// 月额度上限（0=不限制）
    pub monthly_quota_limit: i64,
    /// 当前日窗口已用额度
    pub daily_used_quota: i64,
    /// 当前月窗口已用额度
    pub monthly_used_quota: i64,
    /// 当前日窗口起始时间
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    /// 当前月窗口起始时间
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    /// 最后一次请求时间
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

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
