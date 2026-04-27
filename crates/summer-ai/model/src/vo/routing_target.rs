use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::routing_target::{self, RoutingTargetStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTargetVo {
    pub id: i64,
    pub routing_rule_id: i64,
    pub target_type: String,
    pub channel_id: i64,
    pub account_id: i64,
    pub plugin_id: i64,
    pub target_key: String,
    pub weight: i32,
    pub priority: i32,
    pub cooldown_seconds: i32,
    pub config: serde_json::Value,
    pub status: RoutingTargetStatus,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}

impl RoutingTargetVo {
    pub fn from_model(m: routing_target::Model) -> Self {
        Self {
            id: m.id,
            routing_rule_id: m.routing_rule_id,
            target_type: m.target_type,
            channel_id: m.channel_id,
            account_id: m.account_id,
            plugin_id: m.plugin_id,
            target_key: m.target_key,
            weight: m.weight,
            priority: m.priority,
            cooldown_seconds: m.cooldown_seconds,
            config: m.config,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
