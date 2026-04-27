//! summer-ai-billing
//!
//! 计费引擎：三阶段原子扣费（Reserve → Settle → Refund）+ group_ratio。
//!
//! # 当前状态
//!
//! P0 骨架阶段——空 Plugin，后续 Phase 填内容。

pub mod estimator;
pub mod plugin;
pub mod price;
pub mod service;

pub use estimator::{
    AUDIO_PART_TOKENS, DEFAULT_MAX_COMPLETION, IMAGE_PART_TOKENS, estimate_quota, estimate_usage,
};
pub use plugin::SummerAiBillingPlugin;
pub use price::{
    CostBreakdown, PriceError, PriceResolver, PriceTable, QUOTA_PER_USD, compute_cost,
};
pub use service::{BillingError, BillingService, Reservation, Settlement};
