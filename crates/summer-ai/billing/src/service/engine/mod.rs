use anyhow::Context;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, EntityTrait, ExprTrait, QueryFilter};
use summer::plugin::Service;
use summer_ai_core::types::common::{Message, Usage};
use summer_ai_model::entity::group_ratio;
use summer_ai_model::entity::token;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use crate::service::channel_model_price::{ChannelModelPriceService, ResolvedModelPrice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuotaSettlementPlan {
    charged_quota: i64,
    remain_delta: i64,
    shortfall: i64,
}

#[derive(Clone, Service)]
pub struct BillingEngine {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    channel_model_price_service: ChannelModelPriceService,
}

impl BillingEngine {
    pub fn new(db: DbConn, channel_model_price_service: ChannelModelPriceService) -> Self {
        Self {
            db,
            channel_model_price_service,
        }
    }

    pub async fn resolve_effective_price(
        &self,
        channel_id: i64,
        model_name: &str,
        endpoint_scope: &str,
    ) -> ApiResult<ResolvedModelPrice> {
        self.channel_model_price_service
            .resolve_effective_price(channel_id, model_name, endpoint_scope)
            .await
    }

    pub async fn get_group_ratio(&self, group_code: &str) -> ApiResult<f64> {
        let group = group_ratio::Entity::find()
            .filter(group_ratio::Column::GroupCode.eq(group_code))
            .filter(group_ratio::Column::Enabled.eq(true))
            .one(&self.db)
            .await
            .context("查询分组倍率失败")?;

        Ok(group.map(|item| decimal_to_f64(item.ratio)).unwrap_or(1.0))
    }

    pub async fn pre_consume(
        &self,
        token_id: i64,
        unlimited_quota: bool,
        estimated_tokens: i32,
        input_ratio: f64,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        if unlimited_quota {
            return Ok(0);
        }

        let quota = ((estimated_tokens as f64) * input_ratio * group_ratio).ceil() as i64;
        let quota = std::cmp::max(quota, 1);

        let result = token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                sea_orm::sea_query::Expr::col(token::Column::RemainQuota).sub(quota),
            )
            .filter(token::Column::Id.eq(token_id))
            .filter(token::Column::RemainQuota.gte(quota))
            .exec(&self.db)
            .await
            .context("预扣额度失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::Forbidden("quota exceeded".to_string()));
        }

        Ok(quota)
    }

    pub async fn post_consume(
        &self,
        token_id: i64,
        unlimited_quota: bool,
        pre_consumed: i64,
        usage: &Usage,
        price: &ResolvedModelPrice,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let actual_quota = Self::calculate_actual_quota(usage, price, group_ratio);
        self.settle_consumed_quota(token_id, unlimited_quota, pre_consumed, actual_quota)
            .await
    }

    pub async fn settle_pre_consumed(
        &self,
        token_id: i64,
        unlimited_quota: bool,
        pre_consumed: i64,
    ) -> ApiResult<i64> {
        self.settle_consumed_quota(token_id, unlimited_quota, pre_consumed, pre_consumed)
            .await
    }

    pub async fn refund(&self, token_id: i64, pre_consumed: i64) -> ApiResult<()> {
        if pre_consumed <= 0 {
            return Ok(());
        }

        token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                sea_orm::sea_query::Expr::col(token::Column::RemainQuota).add(pre_consumed),
            )
            .filter(token::Column::Id.eq(token_id))
            .exec(&self.db)
            .await
            .context("退回预扣额度失败")?;

        Ok(())
    }

    pub fn calculate_actual_quota(
        usage: &Usage,
        price: &ResolvedModelPrice,
        group_ratio: f64,
    ) -> i64 {
        let base = usage.prompt_tokens as f64 * price.input_ratio
            + usage.completion_tokens as f64 * price.output_ratio
            + usage.cached_tokens as f64 * price.cached_input_ratio
            + usage.reasoning_tokens as f64 * price.reasoning_ratio;

        (base * group_ratio).ceil() as i64
    }

    pub fn calculate_cost_total(usage: &Usage, price: &ResolvedModelPrice) -> BigDecimal {
        let input = BigDecimal::from(usage.prompt_tokens) * decimal_from_f64(price.input_ratio);
        let output =
            BigDecimal::from(usage.completion_tokens) * decimal_from_f64(price.output_ratio);
        let cached =
            BigDecimal::from(usage.cached_tokens) * decimal_from_f64(price.cached_input_ratio);
        let reasoning =
            BigDecimal::from(usage.reasoning_tokens) * decimal_from_f64(price.reasoning_ratio);

        input + output + cached + reasoning
    }

    pub fn estimate_prompt_tokens(messages: &[Message]) -> i32 {
        let total_chars: usize = messages
            .iter()
            .map(|message| match &message.content {
                serde_json::Value::String(text) => text.chars().count(),
                other => other.to_string().chars().count(),
            })
            .sum();

        (total_chars as f64 / 3.0).ceil() as i32
    }

    pub fn estimate_total_tokens_for_rate_limit(
        messages: &[Message],
        max_tokens: Option<i64>,
    ) -> i64 {
        let prompt_tokens = i64::from(Self::estimate_prompt_tokens(messages));
        prompt_tokens + std::cmp::max(max_tokens.unwrap_or(2048), 1)
    }

    async fn settle_consumed_quota(
        &self,
        token_id: i64,
        unlimited_quota: bool,
        pre_consumed: i64,
        actual_quota: i64,
    ) -> ApiResult<i64> {
        let actual_quota = std::cmp::max(actual_quota, 0);
        if unlimited_quota {
            let result = token::Entity::update_many()
                .col_expr(
                    token::Column::UsedQuota,
                    sea_orm::sea_query::Expr::col(token::Column::UsedQuota).add(actual_quota),
                )
                .filter(token::Column::Id.eq(token_id))
                .exec(&self.db)
                .await
                .context("结算实际额度失败")?;

            if result.rows_affected == 0 {
                return Err(ApiErrors::NotFound(format!("token not found: {token_id}")));
            }

            return Ok(actual_quota);
        }

        for attempt in 0..2 {
            let Some(token_model) = token::Entity::find_by_id(token_id)
                .one(&self.db)
                .await
                .context("查询令牌额度失败")?
            else {
                return Err(ApiErrors::NotFound(format!("token not found: {token_id}")));
            };

            let plan = plan_quota_settlement(pre_consumed, actual_quota, token_model.remain_quota);
            let additional_charge = if plan.remain_delta < 0 {
                -plan.remain_delta
            } else {
                0
            };

            let mut update = token::Entity::update_many().col_expr(
                token::Column::UsedQuota,
                sea_orm::sea_query::Expr::col(token::Column::UsedQuota).add(plan.charged_quota),
            );

            if plan.remain_delta > 0 {
                update = update.col_expr(
                    token::Column::RemainQuota,
                    sea_orm::sea_query::Expr::col(token::Column::RemainQuota)
                        .add(plan.remain_delta),
                );
            } else if plan.remain_delta < 0 {
                update = update
                    .col_expr(
                        token::Column::RemainQuota,
                        sea_orm::sea_query::Expr::col(token::Column::RemainQuota)
                            .sub(additional_charge),
                    )
                    .filter(token::Column::RemainQuota.gte(additional_charge));
            }

            let result = update
                .filter(token::Column::Id.eq(token_id))
                .exec(&self.db)
                .await
                .context("结算实际额度失败")?;

            if result.rows_affected == 1 {
                if plan.shortfall > 0 {
                    tracing::warn!(
                        token_id,
                        actual_quota,
                        pre_consumed,
                        charged_quota = plan.charged_quota,
                        shortfall = plan.shortfall,
                        "settlement quota exceeded available balance, charged capped amount",
                    );
                }
                return Ok(plan.charged_quota);
            }

            if attempt == 1 {
                return Err(ApiErrors::Conflict(
                    "token quota changed during settlement, please retry".to_string(),
                ));
            }
        }

        unreachable!("quota settlement retry loop should always return")
    }
}

fn plan_quota_settlement(
    pre_consumed: i64,
    actual_quota: i64,
    available_remain_quota: i64,
) -> QuotaSettlementPlan {
    let pre_consumed = std::cmp::max(pre_consumed, 0);
    let actual_quota = std::cmp::max(actual_quota, 0);
    let available_remain_quota = std::cmp::max(available_remain_quota, 0);

    if actual_quota <= pre_consumed {
        return QuotaSettlementPlan {
            charged_quota: actual_quota,
            remain_delta: pre_consumed - actual_quota,
            shortfall: 0,
        };
    }

    let additional_needed = actual_quota - pre_consumed;
    let additional_charged = std::cmp::min(additional_needed, available_remain_quota);

    QuotaSettlementPlan {
        charged_quota: pre_consumed + additional_charged,
        remain_delta: -additional_charged,
        shortfall: additional_needed - additional_charged,
    }
}

fn decimal_to_f64(value: sea_orm::prelude::BigDecimal) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(0.0)
}

fn decimal_from_f64(value: f64) -> BigDecimal {
    format!("{value:.10}")
        .parse::<BigDecimal>()
        .unwrap_or_else(|_| BigDecimal::from(0))
}

#[cfg(test)]
mod tests {
    use summer_ai_core::types::common::{Message, Usage};
    use summer_ai_model::entity::channel_model_price::ChannelModelPriceBillingMode;

    use crate::service::channel_model_price::ResolvedModelPrice;

    use super::{BillingEngine, plan_quota_settlement};

    #[test]
    fn calculate_actual_quota_uses_all_usage_dimensions() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_tokens: 30,
            reasoning_tokens: 20,
        };

        let price = sample_price();
        let actual = BillingEngine::calculate_actual_quota(&usage, &price, 1.0);

        assert_eq!(actual, 1195);
    }

    #[test]
    fn calculate_actual_quota_applies_group_ratio_and_ceil() {
        let usage = Usage {
            prompt_tokens: 1,
            completion_tokens: 0,
            total_tokens: 1,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };

        let price = ResolvedModelPrice {
            model_name: "gpt-5.4".to_string(),
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".to_string(),
            input_ratio: 0.3,
            output_ratio: 0.0,
            cached_input_ratio: 0.0,
            reasoning_ratio: 0.0,
            supported_endpoints: vec!["chat".to_string()],
            price_reference: String::new(),
        };

        let actual = BillingEngine::calculate_actual_quota(&usage, &price, 0.5);
        assert_eq!(actual, 1);
    }

    #[test]
    fn calculate_cost_total_preserves_raw_procurement_sum() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_tokens: 30,
            reasoning_tokens: 20,
        };

        let total = BillingEngine::calculate_cost_total(&usage, &sample_price());

        assert_eq!(total.to_string(), "1195.0000000000");
    }

    #[test]
    fn estimate_prompt_tokens_uses_character_count_heuristic() {
        let messages = vec![
            Message {
                role: "user".to_string(),
                content: serde_json::Value::String("Hello".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: serde_json::Value::String("你好".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let estimate = BillingEngine::estimate_prompt_tokens(&messages);
        assert_eq!(estimate, 3);
    }

    #[test]
    fn estimate_total_tokens_for_rate_limit_defaults_to_2048() {
        let messages = vec![Message {
            role: "user".to_string(),
            content: serde_json::Value::String("test".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];

        let total = BillingEngine::estimate_total_tokens_for_rate_limit(&messages, None);
        assert_eq!(total, 2 + 2048);
    }

    #[test]
    fn plan_quota_settlement_caps_extra_charge_to_available_balance() {
        let plan = plan_quota_settlement(100, 160, 20);

        assert_eq!(plan.charged_quota, 120);
        assert_eq!(plan.remain_delta, -20);
        assert_eq!(plan.shortfall, 40);
    }

    #[test]
    fn plan_quota_settlement_refunds_unused_preconsumed_quota() {
        let plan = plan_quota_settlement(100, 60, 999);

        assert_eq!(plan.charged_quota, 60);
        assert_eq!(plan.remain_delta, 40);
        assert_eq!(plan.shortfall, 0);
    }

    fn sample_price() -> ResolvedModelPrice {
        ResolvedModelPrice {
            model_name: "gpt-5.4".to_string(),
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".to_string(),
            input_ratio: 2.5,
            output_ratio: 10.0,
            cached_input_ratio: 1.5,
            reasoning_ratio: 20.0,
            supported_endpoints: vec!["chat".to_string(), "responses".to_string()],
            price_reference: "cmp_live_001".to_string(),
        }
    }
}
