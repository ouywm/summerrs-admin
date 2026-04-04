use crate::entity::{order, redemption, subscription_plan, topup};
use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPlanVo {
    pub id: i64,
    pub plan_code: String,
    pub plan_name: String,
    pub description: String,
    pub status: i16,
    pub billing_cycle: String,
    pub currency: String,
    pub price: String,
    pub quota: i64,
    pub features: serde_json::Value,
    pub limits: serde_json::Value,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}
impl SubscriptionPlanVo {
    pub fn from_model(m: subscription_plan::Model) -> Self {
        Self {
            id: m.id,
            plan_code: m.plan_code,
            plan_name: m.plan_name,
            description: m.description,
            status: m.status,
            billing_cycle: m.billing_cycle,
            currency: m.currency,
            price: m.price.to_string(),
            quota: m.quota,
            features: m.features,
            limits: m.limits,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderVo {
    pub id: i64,
    pub user_id: i64,
    pub order_no: String,
    pub order_type: String,
    pub status: i16,
    pub currency: String,
    pub amount: String,
    pub discount_amount: String,
    pub pay_amount: String,
    pub quota: i64,
    pub paid_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}
impl OrderVo {
    pub fn from_model(m: order::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            order_no: m.order_no,
            order_type: m.order_type,
            status: m.status,
            currency: m.currency,
            amount: m.amount.to_string(),
            discount_amount: m.discount_amount.to_string(),
            pay_amount: m.pay_amount.to_string(),
            quota: m.quota,
            paid_at: m.paid_at,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopupVo {
    pub id: i64,
    pub user_id: i64,
    pub topup_no: String,
    pub topup_type: String,
    pub status: i16,
    pub currency: String,
    pub amount: String,
    pub quota: i64,
    pub paid_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}
impl TopupVo {
    pub fn from_model(m: topup::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            topup_no: m.topup_no,
            topup_type: m.topup_type,
            status: m.status,
            currency: m.currency,
            amount: m.amount.to_string(),
            quota: m.quota,
            paid_at: m.paid_at,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionVo {
    pub id: i64,
    pub code: String,
    pub batch_id: String,
    pub quota: i64,
    pub status: i16,
    pub redeemed_by: i64,
    pub redeemed_at: Option<DateTime<FixedOffset>>,
    pub expires_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}
impl RedemptionVo {
    pub fn from_model(m: redemption::Model) -> Self {
        Self {
            id: m.id,
            code: m.code,
            batch_id: m.batch_id,
            quota: m.quota,
            status: m.status,
            redeemed_by: m.redeemed_by,
            redeemed_at: m.redeemed_at,
            expires_at: m.expires_at,
            create_time: m.create_time,
        }
    }
}
