use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::alerts::alert_silence::{self, AlertSilenceStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertSilenceQuery {
    pub alert_rule_id: Option<i64>,
    pub status: Option<AlertSilenceStatus>,
    pub scope_type: Option<String>,
    pub scope_key: Option<String>,
}

impl From<AlertSilenceQuery> for Condition {
    fn from(req: AlertSilenceQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(alert_rule_id) = req.alert_rule_id {
            condition = condition.add(alert_silence::Column::AlertRuleId.eq(alert_rule_id));
        }
        if let Some(status) = req.status {
            condition = condition.add(alert_silence::Column::Status.eq(status));
        }
        if let Some(scope_type) = req.scope_type {
            condition = condition.add(alert_silence::Column::ScopeType.eq(scope_type));
        }
        if let Some(scope_key) = req.scope_key {
            condition = condition.add(alert_silence::Column::ScopeKey.contains(&scope_key));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlertSilenceReq {
    pub alert_rule_id: i64,
    #[serde(default = "default_scope_type")]
    #[validate(length(min = 1, max = 32))]
    pub scope_type: String,
    #[serde(default)]
    #[validate(length(max = 128))]
    pub scope_key: String,
    #[serde(default)]
    #[validate(length(max = 255))]
    pub reason: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: DateTime<FixedOffset>,
}

fn default_scope_type() -> String {
    "rule".to_string()
}

impl CreateAlertSilenceReq {
    pub fn into_active_model(self, operator: &str) -> alert_silence::ActiveModel {
        let start_time = self
            .start_time
            .unwrap_or_else(|| chrono::Utc::now().fixed_offset());

        alert_silence::ActiveModel {
            alert_rule_id: Set(self.alert_rule_id),
            scope_type: Set(self.scope_type),
            scope_key: Set(self.scope_key),
            reason: Set(self.reason),
            status: Set(AlertSilenceStatus::Active),
            metadata: Set(self.metadata),
            create_by: Set(operator.to_string()),
            start_time: Set(start_time),
            end_time: Set(self.end_time),
            ..Default::default()
        }
    }
}
