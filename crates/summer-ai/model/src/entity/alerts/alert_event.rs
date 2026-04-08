//! AI 告警事件表
//! 对应 sql/ai/alert_event.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=打开 2=已确认 3=已解决 4=忽略
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
pub enum AlertEventStatus {
    /// 打开
    #[sea_orm(num_value = 1)]
    Open = 1,
    /// 已确认
    #[sea_orm(num_value = 2)]
    Acknowledged = 2,
    /// 已解决
    #[sea_orm(num_value = 3)]
    Resolved = 3,
    /// 忽略
    #[sea_orm(num_value = 4)]
    Ignored = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_event")]
pub struct Model {
    /// 事件ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 告警规则ID
    pub alert_rule_id: i64,
    /// 事件编码
    pub event_code: String,
    /// 严重级别
    pub severity: i16,
    /// 状态：1=打开 2=已确认 3=已解决 4=忽略
    pub status: AlertEventStatus,
    /// 来源域
    pub source_domain: String,
    /// 来源对象
    pub source_ref: String,
    /// 标题
    pub title: String,
    /// 详细说明
    #[sea_orm(column_type = "Text")]
    pub detail: String,
    /// 事件载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    /// 首次触发时间
    pub first_triggered_at: DateTimeWithTimeZone,
    /// 最近触发时间
    pub last_triggered_at: DateTimeWithTimeZone,
    /// 确认人
    pub ack_by: String,
    /// 确认时间
    pub ack_time: Option<DateTimeWithTimeZone>,
    /// 解决人
    pub resolved_by: String,
    /// 解决时间
    pub resolved_time: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,

    /// 关联告警规则（多对一，逻辑关联 ai.alert_rule.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "alert_rule_id", to = "id", skip_fk)]
    pub alert_rule: Option<super::alert_rule::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
