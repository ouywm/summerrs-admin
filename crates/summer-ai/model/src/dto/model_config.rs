use crate::entity::billing::model_config::{self, ModelConfigType};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelConfigDto {
    #[validate(length(min = 1, max = 128, message = "模型标识长度必须在1-128之间"))]
    pub model_name: String,
    #[validate(length(min = 1, max = 128, message = "模型显示名长度必须在1-128之间"))]
    pub display_name: String,
    pub model_type: ModelConfigType,
    #[validate(length(min = 1, max = 64, message = "供应商编码长度必须在1-64之间"))]
    pub vendor_code: String,
    pub supported_endpoints: Option<Vec<String>>,
    pub input_ratio: Option<f64>,
    pub output_ratio: Option<f64>,
    pub cached_input_ratio: Option<f64>,
    pub reasoning_ratio: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub max_context: Option<i32>,
    #[validate(length(min = 1, max = 16, message = "货币编码长度必须在1-16之间"))]
    pub currency: Option<String>,
    pub effective_from: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateModelConfigDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_ratio("inputRatio", self.input_ratio)?;
        validate_ratio("outputRatio", self.output_ratio)?;
        validate_ratio("cachedInputRatio", self.cached_input_ratio)?;
        validate_ratio("reasoningRatio", self.reasoning_ratio)?;
        validate_max_context(self.max_context)?;
        validate_currency(self.currency.as_deref())?;
        validate_effective_from(self.effective_from.as_deref())?;
        Ok(())
    }

    pub fn into_active_model(self, operator: &str) -> Result<model_config::ActiveModel, String> {
        Ok(model_config::ActiveModel {
            model_name: Set(self.model_name),
            display_name: Set(self.display_name),
            model_type: Set(self.model_type),
            vendor_code: Set(self.vendor_code),
            supported_endpoints: Set(string_list_json(self.supported_endpoints)),
            input_ratio: Set(decimal_from_f64(self.input_ratio.unwrap_or(1.0))?),
            output_ratio: Set(decimal_from_f64(self.output_ratio.unwrap_or(1.0))?),
            cached_input_ratio: Set(decimal_from_f64(self.cached_input_ratio.unwrap_or(0.0))?),
            reasoning_ratio: Set(decimal_from_f64(self.reasoning_ratio.unwrap_or(0.0))?),
            capabilities: Set(string_list_json(self.capabilities)),
            max_context: Set(self.max_context.unwrap_or(0)),
            currency: Set(self
                .currency
                .unwrap_or_else(|| "USD".to_string())
                .to_ascii_uppercase()),
            effective_from: Set(parse_effective_from(self.effective_from)?),
            metadata: Set(self.metadata.unwrap_or_else(|| serde_json::json!({}))),
            enabled: Set(self.enabled.unwrap_or(true)),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelConfigDto {
    #[validate(length(min = 1, max = 128, message = "模型显示名长度必须在1-128之间"))]
    pub display_name: Option<String>,
    pub model_type: Option<ModelConfigType>,
    #[validate(length(min = 1, max = 64, message = "供应商编码长度必须在1-64之间"))]
    pub vendor_code: Option<String>,
    pub supported_endpoints: Option<Vec<String>>,
    pub input_ratio: Option<f64>,
    pub output_ratio: Option<f64>,
    pub cached_input_ratio: Option<f64>,
    pub reasoning_ratio: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub max_context: Option<i32>,
    #[validate(length(min = 1, max = 16, message = "货币编码长度必须在1-16之间"))]
    pub currency: Option<String>,
    pub effective_from: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateModelConfigDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_ratio("inputRatio", self.input_ratio)?;
        validate_ratio("outputRatio", self.output_ratio)?;
        validate_ratio("cachedInputRatio", self.cached_input_ratio)?;
        validate_ratio("reasoningRatio", self.reasoning_ratio)?;
        validate_max_context(self.max_context)?;
        validate_currency(self.currency.as_deref())?;
        validate_effective_from(self.effective_from.as_deref())?;
        Ok(())
    }

    pub fn apply_to(
        self,
        active: &mut model_config::ActiveModel,
        operator: &str,
    ) -> Result<(), String> {
        active.update_by = Set(operator.to_string());
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
            active.supported_endpoints = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.input_ratio {
            active.input_ratio = Set(decimal_from_f64(v)?);
        }
        if let Some(v) = self.output_ratio {
            active.output_ratio = Set(decimal_from_f64(v)?);
        }
        if let Some(v) = self.cached_input_ratio {
            active.cached_input_ratio = Set(decimal_from_f64(v)?);
        }
        if let Some(v) = self.reasoning_ratio {
            active.reasoning_ratio = Set(decimal_from_f64(v)?);
        }
        if let Some(v) = self.capabilities {
            active.capabilities = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.max_context {
            active.max_context = Set(v);
        }
        if let Some(v) = self.currency {
            active.currency = Set(v.to_ascii_uppercase());
        }
        if self.effective_from.is_some() {
            active.effective_from = Set(parse_effective_from(self.effective_from)?);
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
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigQueryDto {
    pub model_name: Option<String>,
    pub model_type: Option<ModelConfigType>,
    pub vendor_code: Option<String>,
    pub enabled: Option<bool>,
    pub keyword: Option<String>,
}

impl From<ModelConfigQueryDto> for Condition {
    fn from(query: ModelConfigQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.model_name {
            cond = cond.add(model_config::Column::ModelName.eq(v));
        }
        if let Some(v) = query.model_type {
            cond = cond.add(model_config::Column::ModelType.eq(v));
        }
        if let Some(v) = query.vendor_code {
            cond = cond.add(model_config::Column::VendorCode.eq(v));
        }
        if let Some(v) = query.enabled {
            cond = cond.add(model_config::Column::Enabled.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(model_config::Column::ModelName.contains(&keyword))
                        .add(model_config::Column::DisplayName.contains(&keyword))
                        .add(model_config::Column::VendorCode.contains(&keyword))
                        .add(model_config::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn decimal_from_f64(value: f64) -> Result<bigdecimal::BigDecimal, String> {
    bigdecimal::BigDecimal::try_from(value)
        .map_err(|_| format!("无法将数值转换为 Decimal: {value}"))
}

fn validate_ratio(field: &str, value: Option<f64>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{field} 必须是大于等于 0 的有限数"));
    }
    Ok(())
}

fn validate_max_context(value: Option<i32>) -> Result<(), String> {
    if value.is_some_and(|v| v < 0) {
        return Err("maxContext 不能为负数".to_string());
    }
    Ok(())
}

fn validate_currency(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if !value.eq_ignore_ascii_case("USD") {
        return Err("当前仅支持 USD 货币".to_string());
    }
    Ok(())
}

fn validate_effective_from(value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        chrono::DateTime::parse_from_rfc3339(value)
            .map_err(|_| "effectiveFrom 必须是 RFC3339 时间".to_string())?;
    }
    Ok(())
}

fn parse_effective_from(
    value: Option<String>,
) -> Result<Option<sea_orm::prelude::DateTimeWithTimeZone>, String> {
    value
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(&value)
                .map_err(|_| "effectiveFrom 必须是 RFC3339 时间".to_string())
        })
        .transpose()
}

fn string_list_json(values: Option<Vec<String>>) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let trimmed = item.trim();
                (!trimmed.is_empty()).then(|| serde_json::Value::String(trimmed.to_string()))
            })
            .collect(),
    )
}
