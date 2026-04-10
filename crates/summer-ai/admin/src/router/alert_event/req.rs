use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};

use summer_ai_model::entity::alerts::alert_event::{self, AlertEventStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertEventQuery {
    pub alert_rule_id: Option<i64>,
    pub status: Option<AlertEventStatus>,
    pub severity: Option<i16>,
    pub source_domain: Option<String>,
    pub source_ref: Option<String>,
    pub last_triggered_at_start: Option<DateTime<FixedOffset>>,
    pub last_triggered_at_end: Option<DateTime<FixedOffset>>,
}

impl From<AlertEventQuery> for Condition {
    fn from(req: AlertEventQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(alert_rule_id) = req.alert_rule_id {
            condition = condition.add(alert_event::Column::AlertRuleId.eq(alert_rule_id));
        }
        if let Some(status) = req.status {
            condition = condition.add(alert_event::Column::Status.eq(status));
        }
        if let Some(severity) = req.severity {
            condition = condition.add(alert_event::Column::Severity.eq(severity));
        }
        if let Some(source_domain) = req.source_domain {
            condition = condition.add(alert_event::Column::SourceDomain.eq(source_domain));
        }
        if let Some(source_ref) = req.source_ref {
            condition = condition.add(alert_event::Column::SourceRef.contains(&source_ref));
        }
        if let Some(last_triggered_at_start) = req.last_triggered_at_start {
            condition =
                condition.add(alert_event::Column::LastTriggeredAt.gte(last_triggered_at_start));
        }
        if let Some(last_triggered_at_end) = req.last_triggered_at_end {
            condition =
                condition.add(alert_event::Column::LastTriggeredAt.lte(last_triggered_at_end));
        }
        condition
    }
}
