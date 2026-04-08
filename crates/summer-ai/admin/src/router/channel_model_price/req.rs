use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceQuery {
    pub channel_id: Option<i64>,
    pub model_name: Option<String>,
    pub billing_mode: Option<ChannelModelPriceBillingMode>,
    pub status: Option<ChannelModelPriceStatus>,
}

impl From<ChannelModelPriceQuery> for Condition {
    fn from(req: ChannelModelPriceQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(channel_id) = req.channel_id {
            condition = condition.add(channel_model_price::Column::ChannelId.eq(channel_id));
        }
        if let Some(model_name) = req.model_name {
            condition = condition.add(channel_model_price::Column::ModelName.contains(&model_name));
        }
        if let Some(billing_mode) = req.billing_mode {
            condition = condition.add(channel_model_price::Column::BillingMode.eq(billing_mode));
        }
        if let Some(status) = req.status {
            condition = condition.add(channel_model_price::Column::Status.eq(status));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelModelPriceReq {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128))]
    pub model_name: String,
    #[serde(default = "default_billing_mode")]
    pub billing_mode: ChannelModelPriceBillingMode,
    #[serde(default = "default_currency")]
    pub currency: String,
    pub price_config: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelModelPriceReq {
    pub billing_mode: Option<ChannelModelPriceBillingMode>,
    pub currency: Option<String>,
    pub price_config: Option<serde_json::Value>,
    pub status: Option<ChannelModelPriceStatus>,
    pub remark: Option<String>,
}

fn default_billing_mode() -> ChannelModelPriceBillingMode {
    ChannelModelPriceBillingMode::ByToken
}

fn default_currency() -> String {
    "USD".to_string()
}

impl CreateChannelModelPriceReq {
    pub fn into_active_model(
        self,
        operator: &str,
        reference_id: String,
    ) -> channel_model_price::ActiveModel {
        channel_model_price::ActiveModel {
            channel_id: Set(self.channel_id),
            model_name: Set(self.model_name),
            billing_mode: Set(self.billing_mode),
            currency: Set(self.currency),
            price_config: Set(self.price_config),
            reference_id: Set(reference_id),
            status: Set(ChannelModelPriceStatus::Enabled),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdateChannelModelPriceReq {
    pub fn apply_to(self, active: &mut channel_model_price::ActiveModel, operator: &str) {
        if let Some(billing_mode) = self.billing_mode {
            active.billing_mode = Set(billing_mode);
        }
        if let Some(currency) = self.currency {
            active.currency = Set(currency);
        }
        if let Some(price_config) = self.price_config {
            active.price_config = Set(price_config);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
    }
}
