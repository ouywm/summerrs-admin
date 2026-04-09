//! channel_model_price 服务模块

use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use summer_ai_model::entity::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};
use summer_ai_model::entity::model_config::{self, ModelConfigType};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModelPrice {
    pub model_name: String,
    pub billing_mode: ChannelModelPriceBillingMode,
    pub currency: String,
    pub input_ratio: f64,
    pub output_ratio: f64,
    pub cached_input_ratio: f64,
    pub reasoning_ratio: f64,
    pub supported_endpoints: Vec<String>,
    pub price_reference: String,
}

#[derive(Clone, Service)]
pub struct ChannelModelPriceService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelModelPriceService {
    pub fn new(db: DbConn) -> Self {
        Self { db }
    }

    pub async fn resolve_effective_price(
        &self,
        channel_id: i64,
        model_name: &str,
        endpoint_scope: &str,
    ) -> ApiResult<ResolvedModelPrice> {
        let mut resolved = self.load_model_config_price(model_name).await?;
        resolved.ensure_endpoint_supported(endpoint_scope)?;

        let channel_price = channel_model_price::Entity::find()
            .filter(channel_model_price::Column::ChannelId.eq(channel_id))
            .filter(channel_model_price::Column::ModelName.eq(model_name))
            .filter(channel_model_price::Column::Status.eq(ChannelModelPriceStatus::Enabled))
            .one(&self.db)
            .await
            .context("查询渠道模型价格失败")?;

        if let Some(channel_price) = channel_price {
            resolved.billing_mode = channel_price.billing_mode;
            resolved.currency = channel_price.currency;
            resolved.price_reference = channel_price.reference_id;
            apply_price_config_overrides(&mut resolved, &channel_price.price_config)?;
        }

        Ok(resolved)
    }

    async fn load_model_config_price(&self, model_name: &str) -> ApiResult<ResolvedModelPrice> {
        let config = model_config::Entity::find()
            .filter(model_config::Column::ModelName.eq(model_name))
            .filter(model_config::Column::Enabled.eq(true))
            .one(&self.db)
            .await
            .context("查询模型默认价格失败")?
            .ok_or_else(|| {
                ApiErrors::BadRequest(format!("model is not available: {model_name}"))
            })?;

        Ok(ResolvedModelPrice {
            model_name: config.model_name,
            billing_mode: default_billing_mode(config.model_type),
            currency: config.currency,
            input_ratio: decimal_to_f64(config.input_ratio),
            output_ratio: decimal_to_f64(config.output_ratio),
            cached_input_ratio: decimal_to_f64(config.cached_input_ratio),
            reasoning_ratio: decimal_to_f64(config.reasoning_ratio),
            supported_endpoints: json_string_array(&config.supported_endpoints),
            price_reference: String::new(),
        })
    }
}

impl ResolvedModelPrice {
    pub fn ensure_endpoint_supported(&self, endpoint_scope: &str) -> ApiResult<()> {
        if endpoint_scope.is_empty()
            || self.supported_endpoints.is_empty()
            || self
                .supported_endpoints
                .iter()
                .any(|supported| supported == endpoint_scope)
        {
            return Ok(());
        }

        Err(ApiErrors::BadRequest(format!(
            "model {} does not support endpoint: {endpoint_scope}",
            self.model_name
        )))
    }
}

fn default_billing_mode(model_type: ModelConfigType) -> ChannelModelPriceBillingMode {
    match model_type {
        ModelConfigType::Image | ModelConfigType::Audio => {
            ChannelModelPriceBillingMode::ByMediaUnit
        }
        ModelConfigType::Chat | ModelConfigType::Embedding | ModelConfigType::Reasoning => {
            ChannelModelPriceBillingMode::ByToken
        }
    }
}

fn apply_price_config_overrides(
    resolved: &mut ResolvedModelPrice,
    price_config: &serde_json::Value,
) -> ApiResult<()> {
    let Some(map) = price_config.as_object() else {
        return Err(ApiErrors::BadRequest(
            "channel_model_price.price_config must be a JSON object".to_string(),
        ));
    };

    if let Some(value) = first_price_value(map, &["input", "input_ratio"]) {
        resolved.input_ratio = json_value_to_f64("input", value)?;
    }
    if let Some(value) = first_price_value(map, &["output", "output_ratio"]) {
        resolved.output_ratio = json_value_to_f64("output", value)?;
    }
    if let Some(value) = first_price_value(map, &["cache", "cached_input", "cached_input_ratio"]) {
        resolved.cached_input_ratio = json_value_to_f64("cache", value)?;
    }
    if let Some(value) = first_price_value(map, &["reasoning", "reasoning_ratio"]) {
        resolved.reasoning_ratio = json_value_to_f64("reasoning", value)?;
    }

    Ok(())
}

fn first_price_value<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| map.get(*key))
}

fn json_value_to_f64(field_name: &str, value: &serde_json::Value) -> ApiResult<f64> {
    if let Some(number) = value.as_f64() {
        return Ok(number);
    }
    if let Some(number) = value.as_i64() {
        return Ok(number as f64);
    }
    if let Some(number) = value.as_u64() {
        return Ok(number as f64);
    }
    if let Some(number) = value.as_str() {
        return number.parse::<f64>().map_err(|_| {
            ApiErrors::BadRequest(format!(
                "channel_model_price.price_config.{field_name} must be a number"
            ))
        });
    }

    Err(ApiErrors::BadRequest(format!(
        "channel_model_price.price_config.{field_name} must be a number"
    )))
}

fn decimal_to_f64(value: sea_orm::prelude::BigDecimal) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(0.0)
}

fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, MockDatabase};
    use serde_json::json;
    use summer_ai_model::entity::channel_model_price as channel_model_price_entity;
    use summer_ai_model::entity::channel_model_price::{
        ChannelModelPriceBillingMode, ChannelModelPriceStatus,
    };
    use summer_ai_model::entity::model_config::{self, ModelConfigType};

    use super::ChannelModelPriceService;

    #[tokio::test]
    async fn resolve_effective_price_prefers_channel_price() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![sample_model_config("gpt-5.4")]])
            .append_query_results([vec![sample_channel_model_price("gpt-5.4")]])
            .into_connection();

        let service = ChannelModelPriceService::new(db);
        let price = service
            .resolve_effective_price(101, "gpt-5.4", "chat")
            .await
            .expect("channel price should win");

        assert_eq!(price.model_name, "gpt-5.4");
        assert_eq!(price.billing_mode, ChannelModelPriceBillingMode::ByToken);
        assert_eq!(price.currency, "USD");
        assert_eq!(price.input_ratio, 11.5);
        assert_eq!(price.output_ratio, 22.5);
        assert_eq!(price.cached_input_ratio, 3.5);
        assert_eq!(price.reasoning_ratio, 44.5);
        assert_eq!(price.price_reference, "cmp_live_001");
        assert_eq!(price.supported_endpoints, vec!["chat", "responses"]);
    }

    #[tokio::test]
    async fn resolve_effective_price_falls_back_to_model_config() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![sample_model_config("gpt-5.4")]])
            .append_query_results([Vec::<channel_model_price_entity::Model>::new()])
            .into_connection();

        let service = ChannelModelPriceService::new(db);
        let price = service
            .resolve_effective_price(101, "gpt-5.4", "responses")
            .await
            .expect("model_config fallback should work");

        assert_eq!(price.model_name, "gpt-5.4");
        assert_eq!(price.billing_mode, ChannelModelPriceBillingMode::ByToken);
        assert_eq!(price.currency, "USD");
        assert_eq!(price.input_ratio, 1.25);
        assert_eq!(price.output_ratio, 4.5);
        assert_eq!(price.cached_input_ratio, 0.75);
        assert_eq!(price.reasoning_ratio, 8.0);
        assert_eq!(price.price_reference, "");
        assert_eq!(price.supported_endpoints, vec!["chat", "responses"]);
    }

    #[tokio::test]
    async fn resolve_effective_price_rejects_unsupported_endpoint() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![sample_model_config("gpt-5.4")]])
            .append_query_results([Vec::<channel_model_price_entity::Model>::new()])
            .into_connection();

        let service = ChannelModelPriceService::new(db);
        let error = service
            .resolve_effective_price(101, "gpt-5.4", "embeddings")
            .await
            .expect_err("unsupported endpoint should be rejected");

        assert!(error.to_string().contains("embeddings"));
    }

    fn sample_model_config(model_name: &str) -> model_config::Model {
        model_config::Model {
            id: 1,
            model_name: model_name.to_string(),
            display_name: "GPT-5.4".to_string(),
            model_type: ModelConfigType::Chat,
            vendor_code: "openai".to_string(),
            supported_endpoints: json!(["chat", "responses"]),
            input_ratio: dec("1.25"),
            output_ratio: dec("4.5"),
            cached_input_ratio: dec("0.75"),
            reasoning_ratio: dec("8.0"),
            capabilities: json!(["streaming"]),
            max_context: 128_000,
            currency: "USD".to_string(),
            effective_from: None,
            metadata: json!({}),
            enabled: true,
            remark: String::new(),
            create_by: "system".to_string(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: "system".to_string(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    fn sample_channel_model_price(model_name: &str) -> channel_model_price_entity::Model {
        channel_model_price_entity::Model {
            id: 9,
            channel_id: 101,
            model_name: model_name.to_string(),
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".to_string(),
            price_config: json!({
                "input": 11.5,
                "output": 22.5,
                "cache": 3.5,
                "reasoning": 44.5
            }),
            reference_id: "cmp_live_001".to_string(),
            status: ChannelModelPriceStatus::Enabled,
            remark: String::new(),
            create_by: "admin".to_string(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: "admin".to_string(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    fn dec(value: &str) -> sea_orm::prelude::BigDecimal {
        value.parse().expect("valid decimal")
    }
}
