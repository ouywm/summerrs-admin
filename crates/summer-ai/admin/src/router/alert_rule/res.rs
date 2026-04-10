use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::alerts::alert_rule::{self, AlertRuleStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertRuleRes {
    pub id: i64,
    pub domain_code: String,
    pub rule_code: String,
    pub rule_name: String,
    pub severity: i16,
    pub metric_key: String,
    pub condition_expr: String,
    pub threshold_config: serde_json::Value,
    pub channel_config: serde_json::Value,
    pub silence_seconds: i32,
    pub status: AlertRuleStatus,
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl AlertRuleRes {
    pub fn from_model(model: alert_rule::Model) -> Self {
        Self {
            id: model.id,
            domain_code: model.domain_code,
            rule_code: model.rule_code,
            rule_name: model.rule_name,
            severity: model.severity,
            metric_key: model.metric_key,
            condition_expr: model.condition_expr,
            threshold_config: model.threshold_config,
            channel_config: model.channel_config,
            silence_seconds: model.silence_seconds,
            status: model.status,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}
