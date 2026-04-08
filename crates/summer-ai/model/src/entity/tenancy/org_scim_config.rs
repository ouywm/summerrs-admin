//! AI 组织 SCIM 配置表
//! 对应 sql/ai/org_scim_config.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=测试
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
pub enum OrgScimConfigStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 测试
    #[sea_orm(num_value = 3)]
    Testing = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "org_scim_config")]
pub struct Model {
    /// SCIM 配置ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// SCIM 提供方编码（组织内唯一）
    pub provider_code: String,
    /// SCIM 基础地址
    pub base_url: String,
    /// 鉴权方式：bearer/basic
    pub auth_type: String,
    /// SCIM 访问令牌引用
    pub bearer_token_ref: String,
    /// 开通模式：push/pull/bidirectional
    pub provisioning_mode: String,
    /// 同步间隔（分钟）
    pub sync_interval_minutes: i32,
    /// 是否同步用户
    pub user_sync_enabled: bool,
    /// 是否同步组
    pub group_sync_enabled: bool,
    /// 是否启用停用/删除同步
    pub deprovision_enabled: bool,
    /// 状态：1=启用 2=禁用 3=测试
    pub status: OrgScimConfigStatus,
    /// 同步游标/增量锚点
    pub sync_cursor: String,
    /// 最后同步时间
    pub last_sync_at: Option<DateTimeWithTimeZone>,
    /// 协议配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
