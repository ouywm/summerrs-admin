mod email;
mod ip;
mod partial;
mod phone;

use std::sync::Arc;

use crate::{
    config::MaskingRuleConfig,
    error::{Result, ShardingError},
};

pub use email::EmailMasking;
pub use ip::IpMasking;
pub use partial::PartialMasking;
pub use phone::PhoneMasking;

pub trait MaskingAlgorithm: Send + Sync + 'static {
    fn mask(&self, input: &str) -> String;
    fn algorithm_type(&self) -> &str;
}

pub fn build_algorithm(rule: &MaskingRuleConfig) -> Result<Arc<dyn MaskingAlgorithm>> {
    match rule.algorithm.as_str() {
        "phone" => Ok(Arc::new(PhoneMasking)),
        "email" => Ok(Arc::new(EmailMasking)),
        "ip" => Ok(Arc::new(IpMasking)),
        "partial" => Ok(Arc::new(PartialMasking {
            keep_start: rule.show_first,
            keep_end: rule.show_last,
            mask_char: rule.mask_char.chars().next().unwrap_or('*'),
        })),
        other => Err(ShardingError::Config(format!(
            "unsupported masking algorithm `{other}`"
        ))),
    }
}
