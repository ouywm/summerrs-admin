use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::billing::group_ratio;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupRatioVo {
    pub id: i64,
    pub group_code: String,
    pub group_name: String,
    pub ratio: f64,
    pub enabled: bool,
    pub model_whitelist: Vec<String>,
    pub model_blacklist: Vec<String>,
    pub endpoint_scopes: Vec<String>,
    pub fallback_group_code: String,
    pub policy: serde_json::Value,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl GroupRatioVo {
    pub fn from_model(m: group_ratio::Model) -> Self {
        Self {
            id: m.id,
            group_code: m.group_code,
            group_name: m.group_name,
            ratio: ToPrimitive::to_f64(&m.ratio).unwrap_or(0.0),
            enabled: m.enabled,
            model_whitelist: json_string_array(&m.model_whitelist),
            model_blacklist: json_string_array(&m.model_blacklist),
            endpoint_scopes: json_string_array(&m.endpoint_scopes),
            fallback_group_code: m.fallback_group_code,
            policy: m.policy,
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
