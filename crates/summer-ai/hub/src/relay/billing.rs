use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, ExprTrait, QueryFilter};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_core::types::common::Usage;
use summer_ai_model::entity::model_config;
use summer_ai_model::entity::token;
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::token::TokenInfo;

/// Pricing ratios for one model.
#[derive(Debug, Clone)]
pub struct ModelConfigInfo {
    pub model_name: String,
    pub input_ratio: f64,
    pub output_ratio: f64,
    pub cached_input_ratio: f64,
    pub reasoning_ratio: f64,
}

#[derive(Clone, Service)]
pub struct BillingEngine {
    #[inject(component)]
    db: DbConn,
}

impl BillingEngine {
    /// Reserve quota synchronously before sending the upstream request.
    ///
    /// Atomic operation:
    /// `UPDATE ai.token SET remain_quota = remain_quota - ? WHERE id = ? AND remain_quota >= ?`
    pub async fn pre_consume(
        &self,
        token_id: i64,
        estimated_tokens: i32,
        model_input_ratio: f64,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let quota = (estimated_tokens as f64 * model_input_ratio * group_ratio).ceil() as i64;
        let quota = Ord::max(quota, 1);

        use sea_orm::sea_query::Expr;
        let result = token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                Expr::col(token::Column::RemainQuota).sub(quota),
            )
            .filter(token::Column::Id.eq(token_id))
            .filter(token::Column::RemainQuota.gte(quota))
            .exec(&self.db)
            .await
            .context("failed to reserve quota")
            .map_err(ApiErrors::Internal)?;

        if result.rows_affected == 0 {
            let tk = token::Entity::find_by_id(token_id)
                .one(&self.db)
                .await
                .context("failed to query token after quota reservation miss")
                .map_err(ApiErrors::Internal)?;

            if let Some(tk) = tk
                && tk.unlimited_quota
            {
                return Ok(0);
            }
            return Err(ApiErrors::Forbidden("quota exceeded".into()));
        }

        Ok(quota)
    }

    /// Calculate the final quota from actual usage.
    pub fn calculate_actual_quota(
        usage: &Usage,
        model_config: &ModelConfigInfo,
        group_ratio: f64,
    ) -> i64 {
        let base = usage.prompt_tokens as f64 * model_config.input_ratio
            + usage.completion_tokens as f64 * model_config.output_ratio
            + usage.cached_tokens as f64 * model_config.cached_input_ratio
            + usage.reasoning_tokens as f64 * model_config.reasoning_ratio;

        (base * group_ratio).ceil() as i64
    }

    /// Settle quota asynchronously after the upstream request finishes.
    ///
    /// actual_quota = prompt * input_ratio + completion * output_ratio
    ///              + cached * cached_ratio + reasoning * reasoning_ratio
    /// delta = actual - pre_consumed
    pub async fn post_consume(
        &self,
        token_info: &TokenInfo,
        pre_consumed: i64,
        usage: &Usage,
        model_config: &ModelConfigInfo,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let actual_quota = Self::calculate_actual_quota(usage, model_config, group_ratio);
        let delta = actual_quota - pre_consumed;
        let token_id = token_info.token_id;

        use sea_orm::sea_query::Expr;
        let mut update = token::Entity::update_many().col_expr(
            token::Column::UsedQuota,
            Expr::col(token::Column::UsedQuota).add(actual_quota),
        );

        if !token_info.unlimited_quota && delta != 0 {
            update = update.col_expr(
                token::Column::RemainQuota,
                Expr::col(token::Column::RemainQuota).sub(delta),
            );
        }

        update
            .filter(token::Column::Id.eq(token_id))
            .exec(&self.db)
            .await
            .context("post settlement failed")
            .map_err(ApiErrors::Internal)?;

        Ok(actual_quota)
    }

    /// Schedule a refund after a terminal failure.
    pub fn refund_later(&self, token_id: i64, pre_consumed: i64) {
        if pre_consumed <= 0 {
            return;
        }

        let this = self.clone();
        tokio::spawn(async move {
            if let Err(error) = this.refund(token_id, pre_consumed).await {
                tracing::warn!("failed to refund reserved quota asynchronously: {error}");
            }
        });
    }

    /// Refund previously reserved quota after a failed request.
    pub async fn refund(&self, token_id: i64, pre_consumed: i64) -> ApiResult<()> {
        if pre_consumed <= 0 {
            return Ok(());
        }
        use sea_orm::sea_query::Expr;
        token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                Expr::col(token::Column::RemainQuota).add(pre_consumed),
            )
            .filter(token::Column::Id.eq(token_id))
            .exec(&self.db)
            .await
            .context("refund reserved quota failed")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    /// Load pricing ratios for one model.
    pub async fn get_model_config(&self, model_name: &str) -> ApiResult<ModelConfigInfo> {
        let cfg = model_config::Entity::find()
            .filter(model_config::Column::ModelName.eq(model_name))
            .one(&self.db)
            .await
            .context("failed to query model config")
            .map_err(ApiErrors::Internal)?;

        match cfg {
            Some(c) => {
                use std::str::FromStr;
                let to_f64 = |bd: sea_orm::entity::prelude::BigDecimal| {
                    f64::from_str(&bd.to_string()).unwrap_or(1.0)
                };
                Ok(ModelConfigInfo {
                    model_name: c.model_name,
                    input_ratio: to_f64(c.input_ratio),
                    output_ratio: to_f64(c.output_ratio),
                    cached_input_ratio: to_f64(c.cached_input_ratio),
                    reasoning_ratio: to_f64(c.reasoning_ratio),
                })
            }
            None => Ok(ModelConfigInfo {
                model_name: model_name.to_string(),
                input_ratio: 1.0,
                output_ratio: 1.0,
                cached_input_ratio: 0.0,
                reasoning_ratio: 0.0,
            }),
        }
    }

    /// Load the pricing ratio for one token group.
    pub async fn get_group_ratio(&self, group_code: &str) -> ApiResult<f64> {
        use summer_ai_model::entity::group_ratio;

        let gr = group_ratio::Entity::find()
            .filter(group_ratio::Column::GroupCode.eq(group_code))
            .filter(group_ratio::Column::Enabled.eq(true))
            .one(&self.db)
            .await
            .context("failed to query group ratio")
            .map_err(ApiErrors::Internal)?;

        match gr {
            Some(g) => {
                use std::str::FromStr;
                Ok(f64::from_str(&g.ratio.to_string()).unwrap_or(1.0))
            }
            None => Ok(1.0),
        }
    }
}

/// Roughly estimate prompt tokens for pre-reservation.
pub fn estimate_prompt_tokens(messages: &[summer_ai_core::types::common::Message]) -> i32 {
    // A simple approximation: about 1 token per 4 characters.
    let total_chars: usize = messages
        .iter()
        .map(|m| match &m.content {
            serde_json::Value::String(s) => s.len(),
            other => other.to_string().len(),
        })
        .sum();
    (total_chars as f64 / 4.0).ceil() as i32
}
