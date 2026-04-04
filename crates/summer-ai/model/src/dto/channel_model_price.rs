use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::channel_model_price::{self, BillingMode, PriceStatus};

/// 创建渠道模型价格
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelModelPriceDto {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128))]
    pub model_name: String,
    #[serde(default = "default_billing_mode")]
    pub billing_mode: BillingMode,
    #[serde(default = "default_currency")]
    pub currency: String,
    pub price_config: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

fn default_billing_mode() -> BillingMode {
    BillingMode::PerToken
}

fn default_currency() -> String {
    "USD".to_string()
}

impl CreateChannelModelPriceDto {
    pub fn into_active_model(
        self,
        operator: &str,
        reference_id: String,
    ) -> channel_model_price::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        channel_model_price::ActiveModel {
            channel_id: Set(self.channel_id),
            model_name: Set(self.model_name),
            billing_mode: Set(self.billing_mode),
            currency: Set(self.currency),
            price_config: Set(self.price_config),
            reference_id: Set(reference_id),
            status: Set(PriceStatus::Active),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

/// 更新渠道模型价格
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelModelPriceDto {
    pub billing_mode: Option<BillingMode>,
    pub currency: Option<String>,
    pub price_config: Option<serde_json::Value>,
    pub status: Option<PriceStatus>,
    pub remark: Option<String>,
}

impl UpdateChannelModelPriceDto {
    pub fn apply_to(self, active: &mut channel_model_price::ActiveModel, operator: &str) {
        if let Some(v) = self.billing_mode {
            active.billing_mode = Set(v);
        }
        if let Some(v) = self.currency {
            active.currency = Set(v);
        }
        if let Some(v) = self.price_config {
            active.price_config = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询渠道模型价格
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryChannelModelPriceDto {
    pub channel_id: Option<i64>,
    pub model_name: Option<String>,
    pub billing_mode: Option<BillingMode>,
    pub status: Option<PriceStatus>,
}

impl From<QueryChannelModelPriceDto> for sea_orm::Condition {
    fn from(dto: QueryChannelModelPriceDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.channel_id {
            cond = cond.add(channel_model_price::Column::ChannelId.eq(v));
        }
        if let Some(v) = dto.model_name {
            cond = cond.add(channel_model_price::Column::ModelName.contains(&v));
        }
        if let Some(v) = dto.billing_mode {
            cond = cond.add(channel_model_price::Column::BillingMode.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(channel_model_price::Column::Status.eq(v));
        }
        cond
    }
}
