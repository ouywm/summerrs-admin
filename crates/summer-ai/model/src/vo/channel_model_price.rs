use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::channel_model_price::{self, BillingMode, PriceStatus};
use crate::entity::channel_model_price_version::{self, PriceVersionStatus};

/// 渠道模型价格 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceVo {
    pub id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub billing_mode: BillingMode,
    pub currency: String,
    pub price_config: serde_json::Value,
    pub reference_id: String,
    pub status: PriceStatus,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelModelPriceVo {
    pub fn from_model(m: channel_model_price::Model) -> Self {
        Self {
            id: m.id,
            channel_id: m.channel_id,
            model_name: m.model_name,
            billing_mode: m.billing_mode,
            currency: m.currency,
            price_config: m.price_config,
            reference_id: m.reference_id,
            status: m.status,
            remark: m.remark,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

/// 渠道模型价格版本 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceVersionVo {
    pub id: i64,
    pub channel_model_price_id: i64,
    pub channel_id: i64,
    pub model_name: String,
    pub version_no: i32,
    pub reference_id: String,
    pub price_config: serde_json::Value,
    pub effective_start_at: DateTime<FixedOffset>,
    pub effective_end_at: Option<DateTime<FixedOffset>>,
    pub status: PriceVersionStatus,
    pub create_time: DateTime<FixedOffset>,
}

impl ChannelModelPriceVersionVo {
    pub fn from_model(m: channel_model_price_version::Model) -> Self {
        Self {
            id: m.id,
            channel_model_price_id: m.channel_model_price_id,
            channel_id: m.channel_id,
            model_name: m.model_name,
            version_no: m.version_no,
            reference_id: m.reference_id,
            price_config: m.price_config,
            effective_start_at: m.effective_start_at,
            effective_end_at: m.effective_end_at,
            status: m.status,
            create_time: m.create_time,
        }
    }
}

/// 价格详情（含版本历史）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelPriceDetailVo {
    #[serde(flatten)]
    pub price: ChannelModelPriceVo,
    pub versions: Vec<ChannelModelPriceVersionVo>,
}
