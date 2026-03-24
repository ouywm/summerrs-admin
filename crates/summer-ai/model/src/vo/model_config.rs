use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::model_config::{self, ModelType};

/// 模型配置 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigVo {
    pub id: i64,
    pub model_name: String,
    pub display_name: String,
    pub model_type: ModelType,
    pub vendor_code: String,
    pub supported_endpoints: serde_json::Value,
    pub input_ratio: f64,
    pub output_ratio: f64,
    pub cached_input_ratio: f64,
    pub reasoning_ratio: f64,
    pub capabilities: serde_json::Value,
    pub max_context: i32,
    pub currency: String,
    pub enabled: bool,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ModelConfigVo {
    pub fn from_model(m: model_config::Model) -> Self {
        use std::str::FromStr;
        let to_f64 = |bd: sea_orm::entity::prelude::BigDecimal| {
            f64::from_str(&bd.to_string()).unwrap_or(0.0)
        };
        Self {
            id: m.id,
            model_name: m.model_name,
            display_name: m.display_name,
            model_type: m.model_type,
            vendor_code: m.vendor_code,
            supported_endpoints: m.supported_endpoints,
            input_ratio: to_f64(m.input_ratio),
            output_ratio: to_f64(m.output_ratio),
            cached_input_ratio: to_f64(m.cached_input_ratio),
            reasoning_ratio: to_f64(m.reasoning_ratio),
            capabilities: m.capabilities,
            max_context: m.max_context,
            currency: m.currency,
            enabled: m.enabled,
            remark: m.remark,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
