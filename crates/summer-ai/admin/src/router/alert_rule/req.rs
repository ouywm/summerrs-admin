use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::alerts::alert_rule::{self, AlertRuleStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertRuleQuery {
    pub domain_code: Option<String>,
    pub rule_code: Option<String>,
    pub metric_key: Option<String>,
    pub status: Option<AlertRuleStatus>,
    pub severity: Option<i16>,
}

impl From<AlertRuleQuery> for Condition {
    fn from(req: AlertRuleQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(domain_code) = req.domain_code {
            condition = condition.add(alert_rule::Column::DomainCode.eq(domain_code));
        }
        if let Some(rule_code) = req.rule_code {
            condition = condition.add(alert_rule::Column::RuleCode.contains(&rule_code));
        }
        if let Some(metric_key) = req.metric_key {
            condition = condition.add(alert_rule::Column::MetricKey.eq(metric_key));
        }
        if let Some(status) = req.status {
            condition = condition.add(alert_rule::Column::Status.eq(status));
        }
        if let Some(severity) = req.severity {
            condition = condition.add(alert_rule::Column::Severity.eq(severity));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlertRuleReq {
    #[serde(default = "default_domain_code")]
    #[validate(length(min = 1, max = 32))]
    pub domain_code: String,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    #[validate(length(min = 1, max = 128))]
    pub rule_name: String,
    pub severity: i16,
    #[validate(length(min = 1, max = 128))]
    pub metric_key: String,
    #[serde(default)]
    pub condition_expr: String,
    #[serde(default)]
    pub threshold_config: serde_json::Value,
    #[serde(default)]
    pub channel_config: serde_json::Value,
    #[serde(default)]
    pub silence_seconds: i32,
    #[serde(default = "default_rule_status")]
    pub status: AlertRuleStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAlertRuleReq {
    #[validate(length(min = 1, max = 128))]
    pub rule_name: Option<String>,
    pub severity: Option<i16>,
    #[validate(length(min = 1, max = 128))]
    pub metric_key: Option<String>,
    pub condition_expr: Option<String>,
    pub threshold_config: Option<serde_json::Value>,
    pub channel_config: Option<serde_json::Value>,
    pub silence_seconds: Option<i32>,
    pub status: Option<AlertRuleStatus>,
}

fn default_domain_code() -> String {
    "relay".to_string()
}

fn default_rule_status() -> AlertRuleStatus {
    AlertRuleStatus::Enabled
}

impl CreateAlertRuleReq {
    pub fn into_active_model(self, operator: &str) -> alert_rule::ActiveModel {
        alert_rule::ActiveModel {
            domain_code: Set(self.domain_code),
            rule_code: Set(self.rule_code),
            rule_name: Set(self.rule_name),
            severity: Set(self.severity),
            metric_key: Set(self.metric_key),
            condition_expr: Set(self.condition_expr),
            threshold_config: Set(self.threshold_config),
            channel_config: Set(self.channel_config),
            silence_seconds: Set(self.silence_seconds.max(0)),
            status: Set(self.status),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdateAlertRuleReq {
    pub fn apply_to(self, active: &mut alert_rule::ActiveModel, operator: &str) {
        if let Some(rule_name) = self.rule_name {
            active.rule_name = Set(rule_name);
        }
        if let Some(severity) = self.severity {
            active.severity = Set(severity);
        }
        if let Some(metric_key) = self.metric_key {
            active.metric_key = Set(metric_key);
        }
        if let Some(condition_expr) = self.condition_expr {
            active.condition_expr = Set(condition_expr);
        }
        if let Some(threshold_config) = self.threshold_config {
            active.threshold_config = Set(threshold_config);
        }
        if let Some(channel_config) = self.channel_config {
            active.channel_config = Set(channel_config);
        }
        if let Some(silence_seconds) = self.silence_seconds {
            active.silence_seconds = Set(silence_seconds.max(0));
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        active.update_by = Set(operator.to_string());
    }
}
