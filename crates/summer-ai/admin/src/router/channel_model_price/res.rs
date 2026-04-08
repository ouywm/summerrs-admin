use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};
use summer_ai_model::entity::channel_model_price_version::{self, ChannelModelPriceVersionStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceRes {
    pub id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub billing_mode: ChannelModelPriceBillingMode,
    pub currency: String,
    pub price_config: serde_json::Value,
    pub reference_id: String,
    pub status: ChannelModelPriceStatus,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelModelPriceRes {
    pub fn from_model(model: channel_model_price::Model) -> Self {
        Self {
            id: model.id,
            channel_id: model.channel_id,
            model_name: model.model_name,
            billing_mode: model.billing_mode,
            currency: model.currency,
            price_config: model.price_config,
            reference_id: model.reference_id,
            status: model.status,
            remark: model.remark,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceVersionRes {
    pub id: i64,
    pub channel_model_price_id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub version_no: i32,
    pub reference_id: String,
    pub price_config: serde_json::Value,
    pub effective_start_at: DateTime<FixedOffset>,
    pub effective_end_at: Option<DateTime<FixedOffset>>,
    pub status: ChannelModelPriceVersionStatus,
    pub create_time: DateTime<FixedOffset>,
}

impl ChannelModelPriceVersionRes {
    pub fn from_model(model: channel_model_price_version::Model) -> Self {
        Self {
            id: model.id,
            channel_model_price_id: model.channel_model_price_id,
            channel_id: model.channel_id,
            model_name: model.model_name,
            version_no: model.version_no,
            reference_id: model.reference_id,
            price_config: model.price_config,
            effective_start_at: model.effective_start_at,
            effective_end_at: model.effective_end_at,
            status: model.status,
            create_time: model.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceDetailRes {
    #[serde(flatten)]
    pub price: ChannelModelPriceRes,
    pub versions: Vec<ChannelModelPriceVersionRes>,
}
