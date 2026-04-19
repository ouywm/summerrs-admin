//! AI SCIM 用户映射表
//! 对应 sql/ai/scim_user_mapping.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=正常 2=停用 3=待删除
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
pub enum ScimUserMappingStatus {
    /// 正常
    #[sea_orm(num_value = 1)]
    Normal = 1,
    /// 停用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 待删除
    #[sea_orm(num_value = 3)]
    PendingDeletion = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "scim_user_mapping")]
pub struct Model {
    /// 用户映射ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// SCIM 配置ID
    pub scim_config_id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 平台用户ID
    pub user_id: i64,
    /// 外部目录用户ID
    pub external_user_id: String,
    /// 外部目录用户名
    pub external_username: String,
    /// 外部目录邮箱
    pub external_email: String,
    /// 同步方向：push/pull/bidirectional
    pub sync_direction: String,
    /// 状态：1=正常 2=停用 3=待删除
    pub status: ScimUserMappingStatus,
    /// 最近同步内容哈希
    pub last_synced_hash: String,
    /// 最近一次 SCIM 载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub scim_payload: serde_json::Value,
    /// 最后同步时间
    pub last_sync_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
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
