use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::vendor;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VendorRes {
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

impl VendorRes {
    pub fn from_model(model: vendor::Model) -> Self {
        Self {
            id: model.id,
            vendor_code: model.vendor_code,
            vendor_name: model.vendor_name,
            api_style: model.api_style,
            icon: model.icon,
            description: model.description,
            base_url: model.base_url,
            doc_url: model.doc_url,
            metadata: model.metadata,
            vendor_sort: model.vendor_sort,
            enabled: model.enabled,
            remark: model.remark,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}
