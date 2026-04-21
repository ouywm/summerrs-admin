//! 价格表 + 成本计算（P6 T1）。
//!
//! 职责：
//!
//! - 把 `ai.channel_model_price.price_config` 这块 JSONB 反序列化成强类型 [`PriceTable`]；
//! - 给定 [`Usage`] + [`PriceTable`] 算出真实成本（USD）+ quota 整数。
//!
//! # quota 单位约定
//!
//! 沿用 NewAPI / OneAPI 传统：`1 quota = $0.000002`，即
//! [`QUOTA_PER_USD`]` = 500_000`。所有 `ai.user_quota.quota` / `ai.log.quota` 都以
//! 这个整数为单位，避免 BigDecimal 在数据库里做加减法。
//!
//! # price_config JSON schema
//!
//! ```json
//! {
//!   "input_per_million": "3.00",
//!   "output_per_million": "15.00",
//!   "cache_read_per_million": "0.30",
//!   "cache_write_per_million": "3.75",
//!   "reasoning_per_million": null
//! }
//! ```
//!
//! - 所有价格字段都是 **`BigDecimal` 字符串形式**（避开 `f64` 精度损失）。
//! - 除 `input_per_million` / `output_per_million` 外都可选；缺失时按"同 output 单价"或 0 处理。
//! - 单位恒为 **USD per 1M tokens**。多币种支持留到后续 Phase。
//!
//! # 设计取舍
//!
//! - **cache_read 单价直接差价**：relay adapter 已经把各家厂商的 cache hit 映射到
//!   `Usage.prompt_tokens_details.cached_tokens`，cache_read_per_million 在 DB 里就是差价
//!   成交价——不再套用 `CostProfile.cache_read_discount` 二次打折。
//! - **T1 不涉及 group_ratio**：那是后续 Phase 做的用户组倍率，这里只算渠道原价。

use bigdecimal::RoundingMode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, prelude::BigDecimal};
use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_ai_core::Usage;
use summer_ai_model::entity::channels::channel_model_price;
use summer_sea_orm::DbConn;

/// `1 USD = 500_000 quota`（NewAPI 约定）。
pub const QUOTA_PER_USD: i64 = 500_000;

/// 渠道模型价格表（从 `channel_model_price.price_config` JSONB 反序列化而来）。
///
/// 所有字段单位恒为 **USD per 1M tokens**。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceTable {
    /// 普通 input token 单价。
    pub input_per_million: BigDecimal,
    /// 普通 output token 单价。
    pub output_per_million: BigDecimal,
    /// 命中 prompt cache 的 input 单价。缺失 = 不计 cache 折扣，按 input 原价计。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million: Option<BigDecimal>,
    /// 写入 prompt cache 的 input 加价。缺失 = 不加价，按 input 原价计。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million: Option<BigDecimal>,
    /// 推理 token 单价（o1 / Claude extended thinking）。缺失 = 按 output 同价计。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_per_million: Option<BigDecimal>,
}

impl PriceTable {
    /// 从 JSONB 反序列化。
    pub fn from_json(value: &serde_json::Value) -> Result<Self, PriceError> {
        serde_json::from_value(value.clone()).map_err(PriceError::InvalidSchema)
    }
}

/// 价格解析 / 计算相关错误。
#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    /// `price_config` JSONB 格式不符合 [`PriceTable`] schema。
    #[error("price_config schema invalid: {0}")]
    InvalidSchema(#[source] serde_json::Error),

    /// 不支持的计费币种（T1 仅支持 USD）。
    #[error("unsupported currency: {0} (T1 supports USD only)")]
    UnsupportedCurrency(String),

    /// 不支持的计费模式（T1 仅支持 ByToken）。
    #[error("unsupported billing mode: {0:?} (T1 supports ByToken only)")]
    UnsupportedBillingMode(channel_model_price::ChannelModelPriceBillingMode),

    /// 找不到对应 `(channel_id, model_name)` 的生效价格记录。
    #[error("no enabled price row for channel_id={channel_id} model={model}")]
    NotFound {
        /// 找不到的渠道 ID。
        channel_id: i64,
        /// 找不到的模型名。
        model: String,
    },

    /// 数据库错误。
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

/// 一次 usage 的成本明细（含原始 USD 成本 + quota 整数）。
#[derive(Debug, Clone, PartialEq)]
pub struct CostBreakdown {
    /// 该次 usage 的总成本（USD，精度由 BigDecimal 保证）。
    pub cost_usd: BigDecimal,
    /// 折算后的 quota 整数（向上取整，保证平台不亏）。
    pub quota: i64,
    /// 命中的价格快照引用 ID（落到 `ai.log.price_reference`）。
    pub price_reference: String,
}

/// 按 [`Usage`] 明细和 [`PriceTable`] 算出成本。纯函数，无 I/O，便于单测。
///
/// 规则：
///
/// 1. `input_tokens = prompt_tokens - cached_tokens`（未命中 cache 的部分按 input_per_million）；
/// 2. `cached_tokens` 按 cache_read_per_million 结算，缺失时回退到 input_per_million；
/// 3. `reasoning_tokens` 按 reasoning_per_million 结算，缺失时按 output_per_million；
/// 4. 其余 `completion_tokens` 按 output_per_million 结算；
/// 5. `cost_usd = Σ(tokens × price / 1_000_000)`；
/// 6. `quota = ceil(cost_usd × QUOTA_PER_USD)`（向上取整，保证不亏）。
///
/// `reference_id` 由调用方传入（通常来自 `channel_model_price.reference_id`），原样回填进
/// [`CostBreakdown`]。
pub fn compute_cost(usage: &Usage, price: &PriceTable, reference_id: &str) -> CostBreakdown {
    let cached = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens)
        .unwrap_or(0)
        .max(0);
    let reasoning = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens)
        .unwrap_or(0)
        .max(0);

    // input 去掉已命中 cache 的部分；防御性 max(0) 防止上游 usage 不自洽时产负数。
    let billable_input = (usage.prompt_tokens - cached).max(0);
    // completion 去掉 reasoning 部分。
    let billable_output = (usage.completion_tokens - reasoning).max(0);

    let cache_read_price = price
        .cache_read_per_million
        .clone()
        .unwrap_or_else(|| price.input_per_million.clone());
    let reasoning_price = price
        .reasoning_per_million
        .clone()
        .unwrap_or_else(|| price.output_per_million.clone());

    let cost_input = mul_tokens_price(billable_input, &price.input_per_million);
    let cost_cache = mul_tokens_price(cached, &cache_read_price);
    let cost_output = mul_tokens_price(billable_output, &price.output_per_million);
    let cost_reasoning = mul_tokens_price(reasoning, &reasoning_price);

    let cost_usd = cost_input + cost_cache + cost_output + cost_reasoning;
    let quota = usd_to_quota_ceil(&cost_usd);

    CostBreakdown {
        cost_usd,
        quota,
        price_reference: reference_id.to_string(),
    }
}

/// `tokens × price_per_million / 1_000_000`，全程 BigDecimal 无精度损失。
fn mul_tokens_price(tokens: i64, per_million: &BigDecimal) -> BigDecimal {
    if tokens == 0 {
        return BigDecimal::from(0);
    }
    BigDecimal::from(tokens) * per_million / BigDecimal::from(1_000_000_i64)
}

/// USD → quota 整数，向上取整（平台友好型舍入）。
fn usd_to_quota_ceil(cost_usd: &BigDecimal) -> i64 {
    // cost_usd × QUOTA_PER_USD 可能是小数，用 with_scale_round 向上取整。
    let quota_dec = cost_usd * BigDecimal::from(QUOTA_PER_USD);
    let rounded = quota_dec.with_scale_round(0, RoundingMode::Ceiling);
    // BigDecimal → i64；理论上极大金额会溢出，生产场景 Claude 单次最多 ~$10 × 500_000 = 5M，远低于 i64。
    rounded.to_string().parse::<i64>().unwrap_or(i64::MAX)
}

// ---------------------------------------------------------------------------
// PriceResolver —— DB-backed 价格查询服务
// ---------------------------------------------------------------------------

/// 价格解析服务：`(channel_id, model)` → [`PriceTable`]。
///
/// 当前实现为 **每次查 DB**；后续可加 Redis / 进程内 LRU 缓存（参照 `ChannelStore`）。
#[derive(Clone, Service)]
pub struct PriceResolver {
    #[inject(component)]
    db: DbConn,
}

impl PriceResolver {
    /// 按 `(channel_id, model_name)` 找一条 `status = Enabled` 的价格记录，解析成
    /// [`PriceTable`]。
    ///
    /// 返回的 `reference_id` 供落 `ai.log.price_reference` 使用。
    ///
    /// 多条记录时取 `update_time` 最新的那条（兜底防 DBA 误插）。
    pub async fn resolve(
        &self,
        channel_id: i64,
        model: &str,
    ) -> Result<(PriceTable, String), PriceError> {
        let row = channel_model_price::Entity::find()
            .filter(channel_model_price::Column::ChannelId.eq(channel_id))
            .filter(channel_model_price::Column::ModelName.eq(model))
            .filter(
                channel_model_price::Column::Status
                    .eq(channel_model_price::ChannelModelPriceStatus::Enabled),
            )
            .order_by_desc(channel_model_price::Column::UpdateTime)
            .one(&self.db)
            .await?;

        let Some(row) = row else {
            return Err(PriceError::NotFound {
                channel_id,
                model: model.to_string(),
            });
        };

        if row.billing_mode != channel_model_price::ChannelModelPriceBillingMode::ByToken {
            return Err(PriceError::UnsupportedBillingMode(row.billing_mode));
        }
        if !row.currency.eq_ignore_ascii_case("USD") {
            return Err(PriceError::UnsupportedCurrency(row.currency));
        }

        let table = PriceTable::from_json(&row.price_config)?;
        Ok((table, row.reference_id))
    }
}

// ---------------------------------------------------------------------------
// 辅助：把字符串价格（单测场景方便）一把反序列化成 BigDecimal。
// ---------------------------------------------------------------------------

/// 便捷构造（仅用于单测 / 种子数据）。
///
/// ```ignore
/// let p = PriceTable::for_test("3.00", "15.00");
/// ```
#[cfg(any(test, feature = "test-support"))]
impl PriceTable {
    pub fn for_test(input: &str, output: &str) -> Self {
        use std::str::FromStr;
        Self {
            input_per_million: BigDecimal::from_str(input).expect("valid decimal"),
            output_per_million: BigDecimal::from_str(output).expect("valid decimal"),
            cache_read_per_million: None,
            cache_write_per_million: None,
            reasoning_per_million: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::str::FromStr;
    use summer_ai_core::{CompletionTokensDetails, PromptTokensDetails};

    fn price_gpt4o_mini() -> PriceTable {
        PriceTable {
            input_per_million: BigDecimal::from_str("0.15").unwrap(),
            output_per_million: BigDecimal::from_str("0.60").unwrap(),
            cache_read_per_million: Some(BigDecimal::from_str("0.075").unwrap()),
            cache_write_per_million: None,
            reasoning_per_million: None,
        }
    }

    fn usage_simple(prompt: i64, completion: i64) -> Usage {
        Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }
    }

    fn usage_with_cache(prompt: i64, cached: i64, completion: i64) -> Usage {
        Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: Some(cached),
                audio_tokens: None,
            }),
            completion_tokens_details: None,
        }
    }

    #[test]
    fn price_table_parses_minimal_json() {
        let v = json!({
            "input_per_million": "3.00",
            "output_per_million": "15.00"
        });
        let t = PriceTable::from_json(&v).unwrap();
        assert_eq!(t.input_per_million, BigDecimal::from_str("3.00").unwrap());
        assert_eq!(t.output_per_million, BigDecimal::from_str("15.00").unwrap());
        assert!(t.cache_read_per_million.is_none());
    }

    #[test]
    fn price_table_parses_full_json() {
        let v = json!({
            "input_per_million": "3.00",
            "output_per_million": "15.00",
            "cache_read_per_million": "0.30",
            "cache_write_per_million": "3.75",
            "reasoning_per_million": "60.00"
        });
        let t = PriceTable::from_json(&v).unwrap();
        assert_eq!(
            t.cache_read_per_million.unwrap(),
            BigDecimal::from_str("0.30").unwrap()
        );
        assert_eq!(
            t.reasoning_per_million.unwrap(),
            BigDecimal::from_str("60.00").unwrap()
        );
    }

    #[test]
    fn price_table_rejects_missing_required_field() {
        let v = json!({ "input_per_million": "3.00" }); // 缺 output
        let err = PriceTable::from_json(&v).unwrap_err();
        assert!(matches!(err, PriceError::InvalidSchema(_)));
    }

    #[test]
    fn compute_cost_pure_tokens_gpt4o_mini() {
        // 1M input tokens × $0.15 + 2M output × $0.60 = $0.15 + $1.20 = $1.35
        // quota = 1.35 × 500_000 = 675_000
        let usage = usage_simple(1_000_000, 2_000_000);
        let breakdown = compute_cost(&usage, &price_gpt4o_mini(), "ref-x");
        assert_eq!(breakdown.cost_usd, BigDecimal::from_str("1.35").unwrap());
        assert_eq!(breakdown.quota, 675_000);
        assert_eq!(breakdown.price_reference, "ref-x");
    }

    #[test]
    fn compute_cost_with_cache_read_applies_cache_price() {
        // prompt=1M, cached=500k, completion=0
        // billable_input = 500k × 0.15 / 1M = 0.075
        // cache = 500k × 0.075 / 1M = 0.0375
        // total = 0.1125; quota = 56_250
        let usage = usage_with_cache(1_000_000, 500_000, 0);
        let breakdown = compute_cost(&usage, &price_gpt4o_mini(), "ref-c");
        assert_eq!(breakdown.cost_usd, BigDecimal::from_str("0.1125").unwrap());
        assert_eq!(breakdown.quota, 56_250);
    }

    #[test]
    fn compute_cost_cache_price_falls_back_to_input_when_missing() {
        // cache_read_per_million = None → 按 input 原价计
        let mut price = price_gpt4o_mini();
        price.cache_read_per_million = None;
        // prompt=1M, cached=500k：全部按 0.15 算 → 1M × 0.15 / 1M = 0.15
        let usage = usage_with_cache(1_000_000, 500_000, 0);
        let breakdown = compute_cost(&usage, &price, "ref");
        assert_eq!(breakdown.cost_usd, BigDecimal::from_str("0.15").unwrap());
    }

    #[test]
    fn compute_cost_reasoning_uses_separate_price_or_output() {
        // 不设 reasoning_per_million → 按 output 价算
        let price = price_gpt4o_mini();
        let usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 1_000_000,
            total_tokens: 1_000_000,
            prompt_tokens_details: None,
            completion_tokens_details: Some(CompletionTokensDetails {
                reasoning_tokens: Some(400_000),
                audio_tokens: None,
                accepted_prediction_tokens: None,
                rejected_prediction_tokens: None,
            }),
        };
        // 普通 completion = 600_000 × 0.60 / 1M = 0.36
        // reasoning     = 400_000 × 0.60 / 1M = 0.24
        // total = 0.60
        let breakdown = compute_cost(&usage, &price, "r");
        assert_eq!(breakdown.cost_usd, BigDecimal::from_str("0.60").unwrap());

        // 设 reasoning_per_million = 3.00 → 单独价
        let mut price2 = price;
        price2.reasoning_per_million = Some(BigDecimal::from_str("3.00").unwrap());
        let b2 = compute_cost(&usage, &price2, "r2");
        // completion = 0.36; reasoning = 400_000 × 3.00 / 1M = 1.20; total = 1.56
        assert_eq!(b2.cost_usd, BigDecimal::from_str("1.56").unwrap());
    }

    #[test]
    fn compute_cost_zero_tokens_returns_zero() {
        let usage = usage_simple(0, 0);
        let breakdown = compute_cost(&usage, &price_gpt4o_mini(), "r");
        assert_eq!(breakdown.cost_usd, BigDecimal::from(0));
        assert_eq!(breakdown.quota, 0);
    }

    #[test]
    fn compute_cost_rounds_quota_up() {
        // 构造一个会出小数的 usd → 验证向上取整
        // 1 token × $3.00 / 1M = 0.000003 USD → × 500_000 = 1.5 quota → ceil=2
        let price = PriceTable::for_test("3.00", "15.00");
        let usage = usage_simple(1, 0);
        let breakdown = compute_cost(&usage, &price, "r");
        assert_eq!(breakdown.quota, 2);
    }

    #[test]
    fn compute_cost_cached_exceeds_prompt_is_clamped() {
        // 防御性：cached > prompt 时 billable_input 不应为负
        let usage = usage_with_cache(100, 500, 0);
        let breakdown = compute_cost(&usage, &price_gpt4o_mini(), "r");
        // billable_input=0；cache=500 × 0.075 / 1M = 0.0000375 → 向上取整成 19 quota
        // (0.0000375 × 500_000 = 18.75, ceil=19)
        assert_eq!(breakdown.quota, 19);
    }
}
