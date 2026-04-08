//! AI SSO 组映射表
//! 对应 sql/ai/sso_group_mapping.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用
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
pub enum SsoGroupMappingStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "sso_group_mapping")]
pub struct Model {
    /// 组映射ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// SSO 配置ID
    pub sso_config_id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 外部组唯一键
    pub external_group_key: String,
    /// 外部组名称
    pub external_group_name: String,
    /// 映射目标类型：organization/team/project
    pub target_scope_type: String,
    /// 映射目标ID
    pub target_scope_id: i64,
    /// 登录后赋予的角色编码
    pub role_code: String,
    /// 是否自动加入目标范围
    pub auto_join: bool,
    /// 状态：1=启用 2=禁用
    pub status: SsoGroupMappingStatus,
    /// 映射规则配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub mapping_config: serde_json::Value,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
