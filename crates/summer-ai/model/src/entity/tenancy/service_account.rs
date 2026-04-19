//! AI 服务账号表（机器身份/机器人账号）
//! 对应 sql/ai/service_account.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=过期
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
pub enum ServiceAccountStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 过期
    #[sea_orm(num_value = 3)]
    Expired = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "service_account")]
pub struct Model {
    /// 服务账号ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属组织ID
    pub organization_id: i64,
    /// 所属团队ID（可为空）
    pub team_id: Option<i64>,
    /// 所属项目ID（可为空）
    pub project_id: Option<i64>,
    /// 服务账号编码（组织内唯一）
    pub service_code: String,
    /// 服务账号名称
    pub service_name: String,
    /// 状态：1=启用 2=禁用 3=过期
    pub status: ServiceAccountStatus,
    /// 描述
    pub description: String,
    /// 角色列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub role_codes: serde_json::Value,
    /// 允许模型白名单（JSON 数组，空数组=不限制）
    #[sea_orm(column_type = "JsonBinary")]
    pub allowed_models: serde_json::Value,
    /// 服务账号总额度上限（0=不限制）
    pub quota_limit: i64,
    /// 服务账号累计已用额度
    pub used_quota: i64,
    /// 服务账号日额度上限
    pub daily_quota_limit: i64,
    /// 服务账号月额度上限
    pub monthly_quota_limit: i64,
    /// 服务账号累计请求数
    pub request_count: i64,
    /// 最近访问时间
    pub access_time: Option<DateTimeWithTimeZone>,
    /// 过期时间
    pub expires_at: Option<DateTimeWithTimeZone>,
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
