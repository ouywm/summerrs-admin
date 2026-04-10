use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::alerts::alert_event::{self, AlertEventStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertEventRes {
    pub id: i64,
    pub alert_rule_id: i64,
    pub event_code: String,
    pub severity: i16,
    pub status: AlertEventStatus,
    pub source_domain: String,
    pub source_ref: String,
    pub title: String,
    pub detail: String,
    pub payload: serde_json::Value,
    pub first_triggered_at: DateTime<FixedOffset>,
    pub last_triggered_at: DateTime<FixedOffset>,
    pub ack_by: String,
    pub ack_time: Option<DateTime<FixedOffset>>,
    pub resolved_by: String,
    pub resolved_time: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl AlertEventRes {
    pub fn from_model(model: alert_event::Model) -> Self {
        Self {
            id: model.id,
            alert_rule_id: model.alert_rule_id,
            event_code: model.event_code,
            severity: model.severity,
            status: model.status,
            source_domain: model.source_domain,
            source_ref: model.source_ref,
            title: model.title,
            detail: model.detail,
            payload: model.payload,
            first_triggered_at: model.first_triggered_at,
            last_triggered_at: model.last_triggered_at,
            ack_by: model.ack_by,
            ack_time: model.ack_time,
            resolved_by: model.resolved_by,
            resolved_time: model.resolved_time,
            create_time: model.create_time,
        }
    }
}
