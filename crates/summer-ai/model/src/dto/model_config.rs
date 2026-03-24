use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::model_config::{self, ModelType};

/// 创建/更新模型配置
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelConfigDto {
    #[validate(length(min = 1, max = 128, message = "模型名称长度 1-128"))]
    pub model_name: String,
    #[serde(default)]
    pub display_name: String,
    pub model_type: ModelType,
    #[serde(default)]
    pub vendor_code: String,
    #[serde(default)]
    pub supported_endpoints: serde_json::Value,
    #[serde(default = "default_ratio")]
    pub input_ratio: f64,
    #[serde(default = "default_ratio")]
    pub output_ratio: f64,
    #[serde(default)]
    pub cached_input_ratio: f64,
    #[serde(default)]
    pub reasoning_ratio: f64,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    #[serde(default)]
    pub max_context: i32,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub remark: String,
}

fn default_ratio() -> f64 {
    1.0
}
fn default_currency() -> String {
    "USD".into()
}
fn default_true() -> bool {
    true
}

impl CreateModelConfigDto {
    pub fn into_active_model(self, operator: &str) -> model_config::ActiveModel {
        use sea_orm::entity::prelude::BigDecimal;
        use std::str::FromStr;

        let now = chrono::Utc::now().fixed_offset();
        let bd = |v: f64| BigDecimal::from_str(&v.to_string()).unwrap_or_default();

        model_config::ActiveModel {
            model_name: Set(self.model_name),
            display_name: Set(self.display_name),
            model_type: Set(self.model_type),
            vendor_code: Set(self.vendor_code),
            supported_endpoints: Set(self.supported_endpoints),
            input_ratio: Set(bd(self.input_ratio)),
            output_ratio: Set(bd(self.output_ratio)),
            cached_input_ratio: Set(bd(self.cached_input_ratio)),
            reasoning_ratio: Set(bd(self.reasoning_ratio)),
            capabilities: Set(self.capabilities),
            max_context: Set(self.max_context),
            currency: Set(self.currency),
            metadata: Set(self.metadata),
            enabled: Set(self.enabled),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

/// 更新模型配置
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelConfigDto {
    pub display_name: Option<String>,
    pub model_type: Option<ModelType>,
    pub vendor_code: Option<String>,
    pub supported_endpoints: Option<serde_json::Value>,
    pub input_ratio: Option<f64>,
    pub output_ratio: Option<f64>,
    pub cached_input_ratio: Option<f64>,
    pub reasoning_ratio: Option<f64>,
    pub capabilities: Option<serde_json::Value>,
    pub max_context: Option<i32>,
    pub currency: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub remark: Option<String>,
}

impl UpdateModelConfigDto {
    pub fn apply_to(self, active: &mut model_config::ActiveModel, operator: &str) {
        use sea_orm::entity::prelude::BigDecimal;
        use std::str::FromStr;

        let bd = |v: f64| BigDecimal::from_str(&v.to_string()).unwrap_or_default();

        if let Some(v) = self.display_name {
            active.display_name = Set(v);
        }
        if let Some(v) = self.model_type {
            active.model_type = Set(v);
        }
        if let Some(v) = self.vendor_code {
            active.vendor_code = Set(v);
        }
        if let Some(v) = self.supported_endpoints {
            active.supported_endpoints = Set(v);
        }
        if let Some(v) = self.input_ratio {
            active.input_ratio = Set(bd(v));
        }
        if let Some(v) = self.output_ratio {
            active.output_ratio = Set(bd(v));
        }
        if let Some(v) = self.cached_input_ratio {
            active.cached_input_ratio = Set(bd(v));
        }
        if let Some(v) = self.reasoning_ratio {
            active.reasoning_ratio = Set(bd(v));
        }
        if let Some(v) = self.capabilities {
            active.capabilities = Set(v);
        }
        if let Some(v) = self.max_context {
            active.max_context = Set(v);
        }
        if let Some(v) = self.currency {
            active.currency = Set(v);
        }
        if let Some(v) = self.metadata {
            active.metadata = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询模型配置
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryModelConfigDto {
    pub model_name: Option<String>,
    pub vendor_code: Option<String>,
    pub model_type: Option<ModelType>,
    pub enabled: Option<bool>,
}

impl From<QueryModelConfigDto> for sea_orm::Condition {
    fn from(dto: QueryModelConfigDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.model_name {
            cond = cond.add(model_config::Column::ModelName.contains(&v));
        }
        if let Some(v) = dto.vendor_code {
            cond = cond.add(model_config::Column::VendorCode.eq(v));
        }
        if let Some(v) = dto.model_type {
            cond = cond.add(model_config::Column::ModelType.eq(v));
        }
        if let Some(v) = dto.enabled {
            cond = cond.add(model_config::Column::Enabled.eq(v));
        }
        cond
    }
}
