use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::vendor::{self, ApiStyle};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VendorVo {
    pub id: i64,
    pub vendor_code: String,
    pub vendor_name: String,
    pub api_style: ApiStyle,
    pub icon: String,
    pub description: String,
    pub base_url: String,
    pub doc_url: String,
    pub metadata: serde_json::Value,
    pub vendor_sort: i32,
    pub enabled: bool,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl VendorVo {
    pub fn from_model(m: vendor::Model) -> Self {
        Self {
            id: m.id,
            vendor_code: m.vendor_code,
            vendor_name: m.vendor_name,
            api_style: m.api_style,
            icon: m.icon,
            description: m.description,
            base_url: m.base_url,
            doc_url: m.doc_url,
            metadata: m.metadata,
            vendor_sort: m.vendor_sort,
            enabled: m.enabled,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}
