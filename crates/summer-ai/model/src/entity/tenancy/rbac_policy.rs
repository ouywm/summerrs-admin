use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=草稿
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
pub enum RbacPolicyStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 草稿
    #[sea_orm(num_value = 3)]
    Draft = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "rbac_policy")]
pub struct Model {
    /// 策略ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 策略作用域：organization/team/project/service_account
    pub scope_type: String,
    /// 策略作用域对象ID
    pub scope_id: i64,
    /// 策略编码
    pub policy_code: String,
    /// 策略名称
    pub policy_name: String,
    /// 策略类型：role/attribute/custom
    pub policy_type: String,
    /// 策略绑定主体（JSON，如 role/team/service_account 列表）
    #[sea_orm(column_type = "JsonBinary")]
    pub subject_bindings: serde_json::Value,
    /// 当前生效版本ID
    pub current_version_id: i64,
    /// 状态：1=启用 2=禁用 3=草稿
    pub status: RbacPolicyStatus,
    /// 是否系统预置策略
    pub is_system: bool,
    /// 策略说明
    pub description: String,
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
