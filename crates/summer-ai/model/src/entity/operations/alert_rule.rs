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
pub enum AlertRuleStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_rule")]
pub struct Model {
    /// 规则ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 域编码
    pub domain_code: String,
    /// 规则编码
    pub rule_code: String,
    /// 规则名称
    pub rule_name: String,
    /// 严重级别
    pub severity: i16,
    /// 监控指标键
    pub metric_key: String,
    /// 条件表达式
    #[sea_orm(column_type = "Text")]
    pub condition_expr: String,
    /// 阈值配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub threshold_config: serde_json::Value,
    /// 通知渠道配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub channel_config: serde_json::Value,
    /// 默认静默秒数
    pub silence_seconds: i32,
    /// 状态：1=启用 2=禁用
    pub status: AlertRuleStatus,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,

    /// 关联告警事件（一对多）
    #[sea_orm(has_many)]
    /// events
    pub events: HasMany<super::alert_event::Entity>,

    /// 关联静默规则（一对多）
    #[sea_orm(has_many)]
    /// silences
    pub silences: HasMany<super::alert_silence::Entity>,
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
