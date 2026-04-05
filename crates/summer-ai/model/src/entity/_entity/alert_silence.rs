//! AI 告警静默表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 静默状态（1=生效中 2=已结束）
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
pub enum SilenceStatus {
    /// 生效中
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 已结束
    #[sea_orm(num_value = 2)]
    Ended = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_silence")]
pub struct Model {
    /// 静默ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 告警规则ID
    pub alert_rule_id: i64,
    /// 作用域类型
    pub scope_type: String,
    /// 作用域键
    pub scope_key: String,
    /// 静默原因
    pub reason: String,
    /// 状态
    pub status: SilenceStatus,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 创建人
    pub create_by: String,
    /// 开始时间
    pub start_time: DateTimeWithTimeZone,
    /// 结束时间
    pub end_time: DateTimeWithTimeZone,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 关联告警规则（多对一，逻辑关联 ai.alert_rule.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "alert_rule_id", to = "id", skip_fk)]
    pub alert_rule: Option<super::alert_rule::Entity>,
}
