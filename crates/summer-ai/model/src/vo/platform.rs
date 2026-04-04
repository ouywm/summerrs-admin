use crate::entity::config_entry;
use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntryVo {
    pub id: i64,
    pub config_key: String,
    pub config_value: String,
    pub value_type: String,
    pub category: String,
    pub description: String,
    pub status: i16,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}
impl ConfigEntryVo {
    pub fn from_model(m: config_entry::Model) -> Self {
        Self {
            id: m.id,
            config_key: m.config_key,
            config_value: m.config_value,
            value_type: m.value_type,
            category: m.category,
            description: m.description,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
