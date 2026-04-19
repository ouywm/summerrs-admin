//! AI Guardrail 配置表（组织/项目级内容治理开关）
//! 对应 sql/ai/guardrail_config.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_config")]
pub struct Model {
    /// 配置ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 配置作用域：platform/organization/project
    pub scope_type: String,
    /// 组织ID（0=平台级）
    pub organization_id: i64,
    /// 项目ID（0=非项目级）
    pub project_id: i64,
    /// 是否启用
    pub enabled: bool,
    /// 运行模式：enforce/observe
    pub mode: String,
    /// 系统规则配置（JSON，如 jailbreak/pii/secrets/file_types）
    #[sea_orm(column_type = "JsonBinary")]
    pub system_rules: serde_json::Value,
    /// 允许的文件类型列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub allowed_file_types: serde_json::Value,
    /// 文件上传大小上限（MB）
    pub max_file_size_mb: i32,
    /// 命中隐私信息时的动作
    pub pii_action: String,
    /// 命中密钥/凭证时的动作
    pub secret_action: String,
    /// 扩展配置（JSON）
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

    /// 关联 Guardrail 规则（一对多）
    #[sea_orm(has_many)]
    /// rules
    pub rules: HasMany<super::guardrail_rule::Entity>,
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
