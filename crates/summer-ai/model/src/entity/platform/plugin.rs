use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=下线
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
pub enum PluginStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 下线
    #[sea_orm(num_value = 3)]
    Offline = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "plugin")]
pub struct Model {
    /// 插件ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 插件编码
    pub plugin_code: String,
    /// 插件名称
    pub plugin_name: String,
    /// 插件类型：middleware/router/auth/guardrail/logger/tool
    pub plugin_type: String,
    /// 运行时：wasm/lua/http/native
    pub runtime_type: String,
    /// 插件版本
    pub version: String,
    /// 插件入口
    pub entrypoint: String,
    /// 配置契约（JSON Schema）
    #[sea_orm(column_type = "JsonBinary")]
    pub config_schema: serde_json::Value,
    /// 默认配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub default_config: serde_json::Value,
    /// 状态：1=启用 2=禁用 3=下线
    pub status: PluginStatus,
    /// 是否签名校验通过
    pub signed: bool,
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
