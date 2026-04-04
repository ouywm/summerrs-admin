use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::{order, redemption, subscription_plan, topup};

// ─── SubscriptionPlan ───
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubscriptionPlanDto {
    pub plan_code: String,
    pub plan_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_monthly")]
    pub billing_cycle: String,
    #[serde(default = "default_usd")]
    pub currency: String,
    pub price: f64,
    pub quota: i64,
    #[serde(default)]
    pub features: serde_json::Value,
    #[serde(default)]
    pub limits: serde_json::Value,
}
fn default_monthly() -> String {
    "monthly".into()
}
fn default_usd() -> String {
    "USD".into()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionPlanDto {
    pub plan_name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub quota: Option<i64>,
    pub status: Option<i16>,
    pub features: Option<serde_json::Value>,
    pub limits: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuerySubscriptionPlanDto {
    pub status: Option<i16>,
    pub billing_cycle: Option<String>,
}

impl From<QuerySubscriptionPlanDto> for sea_orm::Condition {
    fn from(dto: QuerySubscriptionPlanDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.status {
            c = c.add(subscription_plan::Column::Status.eq(v));
        }
        if let Some(v) = dto.billing_cycle {
            c = c.add(subscription_plan::Column::BillingCycle.eq(v));
        }
        c
    }
}

// ─── Order ───
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryOrderDto {
    pub user_id: Option<i64>,
    pub order_type: Option<String>,
    pub status: Option<i16>,
    pub order_no: Option<String>,
}
impl From<QueryOrderDto> for sea_orm::Condition {
    fn from(dto: QueryOrderDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            c = c.add(order::Column::UserId.eq(v));
        }
        if let Some(v) = dto.order_type {
            c = c.add(order::Column::OrderType.eq(v));
        }
        if let Some(v) = dto.status {
            c = c.add(order::Column::Status.eq(v));
        }
        if let Some(v) = dto.order_no {
            c = c.add(order::Column::OrderNo.eq(v));
        }
        c
    }
}

// ─── Topup ───
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryTopupDto {
    pub user_id: Option<i64>,
    pub status: Option<i16>,
}
impl From<QueryTopupDto> for sea_orm::Condition {
    fn from(dto: QueryTopupDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            c = c.add(topup::Column::UserId.eq(v));
        }
        if let Some(v) = dto.status {
            c = c.add(topup::Column::Status.eq(v));
        }
        c
    }
}

// ─── Redemption ───
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRedemptionBatchDto {
    pub count: usize,
    pub quota: i64,
    #[serde(default)]
    pub batch_id: String,
    pub expires_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryRedemptionDto {
    pub batch_id: Option<String>,
    pub status: Option<i16>,
    pub code: Option<String>,
}
impl From<QueryRedemptionDto> for sea_orm::Condition {
    fn from(dto: QueryRedemptionDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.batch_id {
            c = c.add(redemption::Column::BatchId.eq(v));
        }
        if let Some(v) = dto.status {
            c = c.add(redemption::Column::Status.eq(v));
        }
        if let Some(v) = dto.code {
            c = c.add(redemption::Column::Code.eq(v));
        }
        c
    }
}
