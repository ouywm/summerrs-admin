use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::alert_rule::{self, AlertRuleStatus, AlertSeverity};

/// 创建告警规则
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlertRuleDto {
    #[serde(default = "default_domain")]
    pub domain_code: String,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    pub rule_name: String,
    #[serde(default = "default_severity")]
    pub severity: AlertSeverity,
    pub metric_key: String,
    #[serde(default)]
    pub condition_expr: String,
    #[serde(default)]
    pub threshold_config: serde_json::Value,
    #[serde(default)]
    pub channel_config: serde_json::Value,
    #[serde(default)]
    pub silence_seconds: i32,
}

fn default_domain() -> String {
    "system".to_string()
}
fn default_severity() -> AlertSeverity {
    AlertSeverity::Warning
}

impl CreateAlertRuleDto {
    pub fn into_active_model(self, operator: &str) -> alert_rule::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        alert_rule::ActiveModel {
            domain_code: Set(self.domain_code),
            rule_code: Set(self.rule_code),
            rule_name: Set(self.rule_name),
            severity: Set(self.severity),
            metric_key: Set(self.metric_key),
            condition_expr: Set(self.condition_expr),
            threshold_config: Set(self.threshold_config),
            channel_config: Set(self.channel_config),
            silence_seconds: Set(self.silence_seconds),
            status: Set(AlertRuleStatus::Enabled),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

/// 更新告警规则
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAlertRuleDto {
    pub rule_name: Option<String>,
    pub severity: Option<AlertSeverity>,
    pub metric_key: Option<String>,
    pub condition_expr: Option<String>,
    pub threshold_config: Option<serde_json::Value>,
    pub channel_config: Option<serde_json::Value>,
    pub silence_seconds: Option<i32>,
    pub status: Option<AlertRuleStatus>,
}

impl UpdateAlertRuleDto {
    pub fn apply_to(self, active: &mut alert_rule::ActiveModel, operator: &str) {
        if let Some(v) = self.rule_name {
            active.rule_name = Set(v);
        }
        if let Some(v) = self.severity {
            active.severity = Set(v);
        }
        if let Some(v) = self.metric_key {
            active.metric_key = Set(v);
        }
        if let Some(v) = self.condition_expr {
            active.condition_expr = Set(v);
        }
        if let Some(v) = self.threshold_config {
            active.threshold_config = Set(v);
        }
        if let Some(v) = self.channel_config {
            active.channel_config = Set(v);
        }
        if let Some(v) = self.silence_seconds {
            active.silence_seconds = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询告警规则
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryAlertRuleDto {
    pub domain_code: Option<String>,
    pub metric_key: Option<String>,
    pub severity: Option<AlertSeverity>,
    pub status: Option<AlertRuleStatus>,
}

impl From<QueryAlertRuleDto> for sea_orm::Condition {
    fn from(dto: QueryAlertRuleDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.domain_code {
            cond = cond.add(alert_rule::Column::DomainCode.eq(v));
        }
        if let Some(v) = dto.metric_key {
            cond = cond.add(alert_rule::Column::MetricKey.eq(v));
        }
        if let Some(v) = dto.severity {
            cond = cond.add(alert_rule::Column::Severity.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(alert_rule::Column::Status.eq(v));
        }
        cond
    }
}

/// 查询告警事件
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryAlertEventDto {
    pub alert_rule_id: Option<i64>,
    pub severity: Option<AlertSeverity>,
    pub status: Option<crate::entity::alert_event::AlertEventStatus>,
    pub source_domain: Option<String>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
}

impl From<QueryAlertEventDto> for sea_orm::Condition {
    fn from(dto: QueryAlertEventDto) -> Self {
        use crate::entity::alert_event;
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.alert_rule_id {
            cond = cond.add(alert_event::Column::AlertRuleId.eq(v));
        }
        if let Some(v) = dto.severity {
            cond = cond.add(alert_event::Column::Severity.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(alert_event::Column::Status.eq(v));
        }
        if let Some(v) = dto.source_domain {
            cond = cond.add(alert_event::Column::SourceDomain.eq(v));
        }
        if let Some(v) = dto.start_time {
            cond = cond.add(alert_event::Column::LastTriggeredAt.gte(v));
        }
        if let Some(v) = dto.end_time {
            cond = cond.add(alert_event::Column::LastTriggeredAt.lte(v));
        }
        cond
    }
}

/// 创建告警静默
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlertSilenceDto {
    pub alert_rule_id: i64,
    #[serde(default = "default_scope_type")]
    pub scope_type: String,
    #[serde(default)]
    pub scope_key: String,
    pub reason: String,
    pub end_time: DateTime<FixedOffset>,
}

fn default_scope_type() -> String {
    "rule".to_string()
}

/// 查询日度统计
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryDailyStatsDto {
    pub stats_date_start: Option<chrono::NaiveDate>,
    pub stats_date_end: Option<chrono::NaiveDate>,
    pub user_id: Option<i64>,
    pub project_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub model_name: Option<String>,
}

impl From<QueryDailyStatsDto> for sea_orm::Condition {
    fn from(dto: QueryDailyStatsDto) -> Self {
        use crate::entity::daily_stats;
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.stats_date_start {
            cond = cond.add(daily_stats::Column::StatsDate.gte(v));
        }
        if let Some(v) = dto.stats_date_end {
            cond = cond.add(daily_stats::Column::StatsDate.lte(v));
        }
        if let Some(v) = dto.user_id {
            cond = cond.add(daily_stats::Column::UserId.eq(v));
        }
        if let Some(v) = dto.project_id {
            cond = cond.add(daily_stats::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.channel_id {
            cond = cond.add(daily_stats::Column::ChannelId.eq(v));
        }
        if let Some(v) = dto.account_id {
            cond = cond.add(daily_stats::Column::AccountId.eq(v));
        }
        if let Some(v) = dto.model_name {
            cond = cond.add(daily_stats::Column::ModelName.contains(&v));
        }
        cond
    }
}
