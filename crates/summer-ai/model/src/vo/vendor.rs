use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::vendor;

/// 供应商 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VendorVo {
    pub id: i64,
    pub vendor_code: String,
    pub vendor_name: String,
    pub api_style: String,
    pub icon: String,
    pub description: String,
    pub base_url: String,
    pub doc_url: String,
    pub metadata: serde_json::Value,
    pub vendor_sort: i32,
    pub enabled: bool,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
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
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
