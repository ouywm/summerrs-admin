use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::ability;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AbilityVo {
    pub id: i64,
    pub channel_group: String,
    pub endpoint_scope: String,
    pub model: String,
    pub channel_id: i64,
    pub enabled: bool,
    pub priority: i32,
    pub weight: i32,
    pub route_config: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}

impl AbilityVo {
    pub fn from_model(m: ability::Model) -> Self {
        Self {
            id: m.id,
            channel_group: m.channel_group,
            endpoint_scope: m.endpoint_scope,
            model: m.model,
            channel_id: m.channel_id,
            enabled: m.enabled,
            priority: m.priority,
            weight: m.weight,
            route_config: m.route_config,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
