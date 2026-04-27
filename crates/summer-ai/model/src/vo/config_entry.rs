use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::platform::config_entry::{self, ConfigEntryStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntryVo {
    pub id: i64,
    pub scope_type: String,
    pub scope_id: i64,
    pub category: String,
    pub config_key: String,
    pub config_value: serde_json::Value,
    pub secret_ref: String,
    pub status: ConfigEntryStatus,
    pub version_no: i32,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl ConfigEntryVo {
    pub fn from_model(m: config_entry::Model) -> Self {
        Self {
            id: m.id,
            scope_type: m.scope_type,
            scope_id: m.scope_id,
            category: m.category,
            config_key: m.config_key,
            config_value: m.config_value,
            secret_ref: m.secret_ref,
            status: m.status,
            version_no: m.version_no,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}
