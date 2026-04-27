use crate::dto::channel_model_price::ChannelModelPriceConfig;
use crate::entity::routing::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};
use crate::entity::routing::channel_model_price_version::{self, ChannelModelPriceVersionStatus};
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceVo {
    pub id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub billing_mode: ChannelModelPriceBillingMode,
    pub currency: String,
    pub price_config: ChannelModelPriceConfig,
    pub reference_id: String,
    pub status: ChannelModelPriceStatus,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl ChannelModelPriceVo {
    pub fn from_model(model: channel_model_price::Model) -> Self {
        let price_config = ChannelModelPriceConfig::from_json(&model.price_config)
            .expect("channel_model_price.price_config must be valid");
        Self {
            id: model.id,
            channel_id: model.channel_id,
            model_name: model.model_name,
            billing_mode: model.billing_mode,
            currency: model.currency,
            price_config,
            reference_id: model.reference_id,
            status: model.status,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceDetailVo {
    #[serde(flatten)]
    pub base: ChannelModelPriceVo,
    pub current_version_no: Option<i32>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceVersionVo {
    pub id: i64,
    pub channel_model_price_id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub version_no: i32,
    pub reference_id: String,
    pub price_config: ChannelModelPriceConfig,
    pub effective_start_at: DateTimeWithTimeZone,
    pub effective_end_at: Option<DateTimeWithTimeZone>,
    pub status: ChannelModelPriceVersionStatus,
    pub create_time: DateTimeWithTimeZone,
}

impl ChannelModelPriceVersionVo {
    pub fn from_model(model: channel_model_price_version::Model) -> Self {
        let price_config = ChannelModelPriceConfig::from_json(&model.price_config)
            .expect("channel_model_price_version.price_config must be valid");
        Self {
            id: model.id,
            channel_model_price_id: model.channel_model_price_id,
            channel_id: model.channel_id,
            model_name: model.model_name,
            version_no: model.version_no,
            reference_id: model.reference_id,
            price_config,
            effective_start_at: model.effective_start_at,
            effective_end_at: model.effective_end_at,
            status: model.status,
            create_time: model.create_time,
        }
    }
}
