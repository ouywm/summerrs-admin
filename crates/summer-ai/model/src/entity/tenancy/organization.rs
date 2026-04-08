//! AI 组织表（多租户组织根实体）
//! 对应 sql/ai/organization.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=归档
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
pub enum OrganizationStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 归档
    #[sea_orm(num_value = 3)]
    Archived = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "organization")]
pub struct Model {
    /// 组织ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织编码（唯一）
    pub org_code: String,
    /// 组织名称
    pub org_name: String,
    /// 组织拥有者用户ID
    pub owner_user_id: i64,
    /// 状态：1=启用 2=禁用 3=归档
    pub status: OrganizationStatus,
    /// 默认用户分组
    pub default_group_code: String,
    /// 账单通知邮箱
    pub billing_email: String,
    /// 计费模式：wallet/subscription/hybrid
    pub billing_mode: String,
    /// 组织总额度上限（0=不限制）
    pub quota_limit: i64,
    /// 组织累计已用额度
    pub used_quota: i64,
    /// 组织日额度上限
    pub daily_quota_limit: i64,
    /// 组织月额度上限
    pub monthly_quota_limit: i64,
    /// 组织累计请求数
    pub request_count: i64,
    /// 组织级设置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub settings: serde_json::Value,
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
