use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::alerts::alert_silence::{self, AlertSilenceStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertSilenceRes {
    pub id: i64,
    pub alert_rule_id: i64,
    pub scope_type: String,
    pub scope_key: String,
    pub reason: String,
    pub status: AlertSilenceStatus,
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: DateTime<FixedOffset>,
    pub create_time: DateTime<FixedOffset>,
}

impl AlertSilenceRes {
    pub fn from_model(model: alert_silence::Model) -> Self {
        Self {
            id: model.id,
            alert_rule_id: model.alert_rule_id,
            scope_type: model.scope_type,
            scope_key: model.scope_key,
            reason: model.reason,
            status: model.status,
            metadata: model.metadata,
            create_by: model.create_by,
            start_time: model.start_time,
            end_time: model.end_time,
            create_time: model.create_time,
        }
    }
}
