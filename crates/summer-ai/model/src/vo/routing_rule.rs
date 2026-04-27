use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::routing_rule::{self, RoutingRuleStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRuleVo {
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub priority: i32,
    pub match_type: String,
    pub match_conditions: serde_json::Value,
    pub route_strategy: String,
    pub fallback_strategy: String,
    pub status: RoutingRuleStatus,
    pub start_time: Option<DateTimeWithTimeZone>,
    pub end_time: Option<DateTimeWithTimeZone>,
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl RoutingRuleVo {
    pub fn from_model(m: routing_rule::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            project_id: m.project_id,
            rule_code: m.rule_code,
            rule_name: m.rule_name,
            priority: m.priority,
            match_type: m.match_type,
            match_conditions: m.match_conditions,
            route_strategy: m.route_strategy,
            fallback_strategy: m.fallback_strategy,
            status: m.status,
            start_time: m.start_time,
            end_time: m.end_time,
            metadata: m.metadata,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}
