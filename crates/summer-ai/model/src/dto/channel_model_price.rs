use crate::entity::routing::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ChannelModelPriceConfig {
    pub input_per_million: String,
    pub output_per_million: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_per_million: Option<String>,
}

impl ChannelModelPriceConfig {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        let parsed: Self =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_decimal("input_per_million", &self.input_per_million)?;
        validate_decimal("output_per_million", &self.output_per_million)?;
        if let Some(value) = &self.cache_read_per_million {
            validate_decimal("cache_read_per_million", value)?;
        }
        if let Some(value) = &self.cache_write_per_million {
            validate_decimal("cache_write_per_million", value)?;
        }
        if let Some(value) = &self.reasoning_per_million {
            validate_decimal("reasoning_per_million", value)?;
        }
        Ok(())
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("ChannelModelPriceConfig should always serialize")
    }
}

fn validate_decimal(field: &str, value: &str) -> Result<(), String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(format!("{field} 不能为空"));
    }
    let decimal = normalized
        .parse::<bigdecimal::BigDecimal>()
        .map_err(|_| format!("{field} 必须是合法小数"))?;
    if decimal < 0 {
        return Err(format!("{field} 不能为负数"));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PriceMutationFingerprint {
    pub billing_mode: ChannelModelPriceBillingMode,
    pub currency: String,
    pub price_config: serde_json::Value,
}

impl PriceMutationFingerprint {
    pub fn from_model(model: &channel_model_price::Model) -> Self {
        Self {
            billing_mode: model.billing_mode,
            currency: model.currency.clone(),
            price_config: model.price_config.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelModelPriceDto {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128, message = "模型名长度必须在1-128之间"))]
    pub model_name: String,
    pub billing_mode: ChannelModelPriceBillingMode,
    #[validate(length(min = 1, max = 16, message = "货币编码长度必须在1-16之间"))]
    pub currency: String,
    pub price_config: serde_json::Value,
    pub status: Option<ChannelModelPriceStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateChannelModelPriceDto {
    pub fn validate_price_config(&self) -> Result<ChannelModelPriceConfig, String> {
        ChannelModelPriceConfig::from_json(&self.price_config)
    }

    pub fn validate_runtime_compatibility(&self) -> Result<(), String> {
        if self.billing_mode != ChannelModelPriceBillingMode::ByToken {
            return Err("当前仅支持按 Token 计费模式".to_string());
        }
        if !self.currency.eq_ignore_ascii_case("USD") {
            return Err("当前仅支持 USD 货币".to_string());
        }
        self.validate_price_config()?;
        Ok(())
    }

    pub fn into_active_model(
        self,
        operator: &str,
        reference_id: String,
    ) -> channel_model_price::ActiveModel {
        channel_model_price::ActiveModel {
            channel_id: Set(self.channel_id),
            model_name: Set(self.model_name),
            billing_mode: Set(self.billing_mode),
            currency: Set(self.currency.to_ascii_uppercase()),
            price_config: Set(self.price_config),
            reference_id: Set(reference_id),
            status: Set(self.status.unwrap_or(ChannelModelPriceStatus::Enabled)),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelModelPriceDto {
    pub billing_mode: Option<ChannelModelPriceBillingMode>,
    #[validate(length(min = 1, max = 16, message = "货币编码长度必须在1-16之间"))]
    pub currency: Option<String>,
    pub price_config: Option<serde_json::Value>,
    pub status: Option<ChannelModelPriceStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateChannelModelPriceDto {
    pub fn touches_price_fields(&self, current: &PriceMutationFingerprint) -> bool {
        self.billing_mode
            .is_some_and(|value| value != current.billing_mode)
            || self
                .currency
                .as_ref()
                .is_some_and(|value| !value.eq_ignore_ascii_case(&current.currency))
            || self
                .price_config
                .as_ref()
                .is_some_and(|value| value != &current.price_config)
    }

    pub fn merged_fingerprint(
        &self,
        current: &PriceMutationFingerprint,
    ) -> PriceMutationFingerprint {
        PriceMutationFingerprint {
            billing_mode: self.billing_mode.unwrap_or(current.billing_mode),
            currency: self
                .currency
                .clone()
                .unwrap_or_else(|| current.currency.clone())
                .to_ascii_uppercase(),
            price_config: self
                .price_config
                .clone()
                .unwrap_or_else(|| current.price_config.clone()),
        }
    }

    pub fn validate_runtime_compatibility(
        &self,
        current: &PriceMutationFingerprint,
    ) -> Result<(), String> {
        let merged = self.merged_fingerprint(current);
        if merged.billing_mode != ChannelModelPriceBillingMode::ByToken {
            return Err("当前仅支持按 Token 计费模式".to_string());
        }
        if !merged.currency.eq_ignore_ascii_case("USD") {
            return Err("当前仅支持 USD 货币".to_string());
        }
        ChannelModelPriceConfig::from_json(&merged.price_config)?;
        Ok(())
    }

    pub fn apply_to(
        self,
        active: &mut channel_model_price::ActiveModel,
        operator: &str,
        reference_id: Option<String>,
    ) {
        active.update_by = Set(operator.to_string());
        if let Some(billing_mode) = self.billing_mode {
            active.billing_mode = Set(billing_mode);
        }
        if let Some(currency) = self.currency {
            active.currency = Set(currency.to_ascii_uppercase());
        }
        if let Some(price_config) = self.price_config {
            active.price_config = Set(price_config);
        }
        if let Some(reference_id) = reference_id {
            active.reference_id = Set(reference_id);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceQueryDto {
    pub channel_id: Option<i64>,
    pub model_name: Option<String>,
    pub status: Option<ChannelModelPriceStatus>,
    pub billing_mode: Option<ChannelModelPriceBillingMode>,
    pub currency: Option<String>,
    pub keyword: Option<String>,
}

impl From<ChannelModelPriceQueryDto> for Condition {
    fn from(query: ChannelModelPriceQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(channel_id) = query.channel_id {
            cond = cond.add(channel_model_price::Column::ChannelId.eq(channel_id));
        }
        if let Some(model_name) = query.model_name {
            cond = cond.add(channel_model_price::Column::ModelName.eq(model_name));
        }
        if let Some(status) = query.status {
            cond = cond.add(channel_model_price::Column::Status.eq(status));
        }
        if let Some(billing_mode) = query.billing_mode {
            cond = cond.add(channel_model_price::Column::BillingMode.eq(billing_mode));
        }
        if let Some(currency) = query.currency {
            cond =
                cond.add(channel_model_price::Column::Currency.eq(currency.to_ascii_uppercase()));
        }
        if let Some(keyword) = query.keyword {
            let keyword_cond = Condition::any()
                .add(channel_model_price::Column::ModelName.contains(&keyword))
                .add(channel_model_price::Column::ReferenceId.contains(&keyword))
                .add(channel_model_price::Column::Remark.contains(&keyword));
            cond = cond.add(keyword_cond);
        }
        cond
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn price_table_rejects_missing_output_price() {
        let err = ChannelModelPriceConfig::from_json(&json!({
            "input_per_million": "3.00"
        }))
        .unwrap_err();

        assert!(err.contains("output_per_million"));
    }

    #[test]
    fn create_dto_accepts_valid_price_config() {
        let dto = CreateChannelModelPriceDto {
            channel_id: 7,
            model_name: "gpt-5.4".to_string(),
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".to_string(),
            price_config: json!({
                "input_per_million": "3.00",
                "output_per_million": "15.00",
                "cache_read_per_million": "0.30",
                "cache_write_per_million": "3.75"
            }),
            status: Some(ChannelModelPriceStatus::Enabled),
            remark: Some("official".to_string()),
        };

        let parsed = dto.validate_price_config().unwrap();
        assert_eq!(parsed.input_per_million, "3.00");
        assert_eq!(parsed.output_per_million, "15.00");
        assert_eq!(parsed.cache_read_per_million.as_deref(), Some("0.30"));
    }

    #[test]
    fn update_dto_detects_billing_field_changes() {
        let current = PriceMutationFingerprint {
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".to_string(),
            price_config: json!({
                "input_per_million": "3.00",
                "output_per_million": "15.00"
            }),
        };

        let no_change = UpdateChannelModelPriceDto {
            billing_mode: None,
            currency: None,
            price_config: None,
            status: Some(ChannelModelPriceStatus::Disabled),
            remark: Some("temporarily disabled".to_string()),
        };
        assert!(!no_change.touches_price_fields(&current));

        let price_change = UpdateChannelModelPriceDto {
            billing_mode: None,
            currency: None,
            price_config: Some(json!({
                "input_per_million": "3.50",
                "output_per_million": "15.00"
            })),
            status: None,
            remark: None,
        };
        assert!(price_change.touches_price_fields(&current));
    }
}
