//! AI Guardrail 规则表（自定义/系统内容治理规则）
//! 对应 sql/ai/guardrail_rule.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 严重级别：1=低 2=中 3=高
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
pub enum GuardrailRuleSeverity {
    /// 低
    #[sea_orm(num_value = 1)]
    Low = 1,
    /// 中
    #[sea_orm(num_value = 2)]
    Medium = 2,
    /// 高
    #[sea_orm(num_value = 3)]
    High = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_rule")]
pub struct Model {
    /// 规则ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属 Guardrail 配置ID
    pub guardrail_config_id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 团队ID
    pub team_id: i64,
    /// 令牌ID（0=不绑定）
    pub token_id: i64,
    /// 服务账号ID（0=不绑定）
    pub service_account_id: i64,
    /// 规则编码
    pub rule_code: String,
    /// 规则名称
    pub rule_name: String,
    /// 规则类型：blocked_terms/custom_regex/topic_restriction/pii/prompt_injection/file_types 等
    pub rule_type: String,
    /// 执行阶段：request_input/response_output/file_upload/tool_result/system_prompt
    pub phase: String,
    /// 命中后的动作：allow/block/redact/warn/quarantine
    pub action: String,
    /// 优先级（越大越先执行）
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
    /// 严重级别：1=低 2=中 3=高
    pub severity: GuardrailRuleSeverity,
    /// 模型匹配模式
    pub model_pattern: String,
    /// Endpoint 匹配模式
    pub endpoint_pattern: String,
    /// 附加条件（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub condition_json: serde_json::Value,
    /// 规则配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub rule_config: serde_json::Value,
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

    /// 关联 Guardrail 配置（多对一，逻辑关联 ai.guardrail_config.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "guardrail_config_id", to = "id", skip_fk)]
    /// guardrail config
    pub guardrail_config: Option<super::guardrail_config::Entity>,
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
