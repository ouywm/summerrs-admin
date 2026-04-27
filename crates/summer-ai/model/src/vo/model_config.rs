use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::billing::model_config::{self, ModelConfigType};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigVo {
    pub id: i64,
    pub model_name: String,
    pub display_name: String,
    pub model_type: ModelConfigType,
    pub vendor_code: String,
    pub supported_endpoints: Vec<String>,
    pub input_ratio: f64,
    pub output_ratio: f64,
    pub cached_input_ratio: f64,
    pub reasoning_ratio: f64,
    pub capabilities: Vec<String>,
    pub max_context: i32,
    pub currency: String,
    pub effective_from: Option<DateTimeWithTimeZone>,
    pub metadata: serde_json::Value,
    pub enabled: bool,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl ModelConfigVo {
    pub fn from_model(m: model_config::Model) -> Self {
        Self {
            id: m.id,
            model_name: m.model_name,
            display_name: m.display_name,
            model_type: m.model_type,
            vendor_code: m.vendor_code,
            supported_endpoints: json_string_array(&m.supported_endpoints),
            input_ratio: ToPrimitive::to_f64(&m.input_ratio).unwrap_or(0.0),
            output_ratio: ToPrimitive::to_f64(&m.output_ratio).unwrap_or(0.0),
            cached_input_ratio: ToPrimitive::to_f64(&m.cached_input_ratio).unwrap_or(0.0),
            reasoning_ratio: ToPrimitive::to_f64(&m.reasoning_ratio).unwrap_or(0.0),
            capabilities: json_string_array(&m.capabilities),
            max_context: m.max_context,
            currency: m.currency,
            effective_from: m.effective_from,
            metadata: m.metadata,
            enabled: m.enabled,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}

fn json_string_array(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
