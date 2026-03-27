mod complex;
mod hash_mod;
mod hash_range;
mod inline;
mod tenant;
mod time_range;

use std::sync::Arc;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::{TableRuleConfig, TenantIsolationLevel},
    error::{Result, ShardingError},
};

pub use complex::ComplexShardingAlgorithm;
pub use hash_mod::HashModShardingAlgorithm;
pub use hash_range::HashRangeShardingAlgorithm;
pub use inline::InlineShardingAlgorithm;
pub use tenant::TenantShardingAlgorithm;
pub use time_range::{TimeGranularity, TimeRangeShardingAlgorithm};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShardingValue {
    Int(i64),
    Str(String),
    DateTime(DateTime<FixedOffset>),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeBound {
    pub value: ShardingValue,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShardingCondition {
    Exact(ShardingValue),
    Range {
        lower: Option<RangeBound>,
        upper: Option<RangeBound>,
    },
}

pub trait ShardingAlgorithm: Send + Sync + 'static {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String>;

    fn do_range_sharding(
        &self,
        available_targets: &[String],
        lower: &ShardingValue,
        upper: &ShardingValue,
    ) -> Vec<String>;

    fn algorithm_type(&self) -> &str;
}

#[derive(Debug, Default, Clone)]
pub struct AlgorithmRegistry;

impl AlgorithmRegistry {
    pub fn build(&self, rule: &TableRuleConfig) -> Result<Arc<dyn ShardingAlgorithm>> {
        match rule.algorithm.as_str() {
            "hash_mod" => Ok(Arc::new(HashModShardingAlgorithm::from_rule(rule)?)),
            "time_range" => Ok(Arc::new(TimeRangeShardingAlgorithm::from_rule(rule)?)),
            "hash_range" => Ok(Arc::new(HashRangeShardingAlgorithm::default())),
            "inline" => Ok(Arc::new(InlineShardingAlgorithm::from_rule(rule)?)),
            "tenant" => Ok(Arc::new(TenantShardingAlgorithm::default())),
            "complex" => Ok(Arc::new(ComplexShardingAlgorithm::from_rule(rule)?)),
            other => Err(ShardingError::Config(format!(
                "unsupported sharding algorithm `{other}`"
            ))),
        }
    }
}

impl ShardingValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Str(value) => value.parse().ok(),
            Self::DateTime(value) => Some(value.timestamp_millis()),
            Self::Null => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn as_datetime(&self) -> Option<DateTime<FixedOffset>> {
        match self {
            Self::DateTime(value) => Some(*value),
            Self::Str(value) => parse_datetime_string(value),
            _ => None,
        }
    }
}

pub fn parse_datetime_string(value: &str) -> Option<DateTime<FixedOffset>> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime);
    }
    if let Ok(datetime) = DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%#z") {
        return Some(datetime);
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return FixedOffset::east_opt(0)
            .and_then(|offset| offset.from_local_datetime(&datetime).single());
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let datetime = date.and_hms_opt(0, 0, 0)?;
        return FixedOffset::east_opt(0)
            .and_then(|offset| offset.from_local_datetime(&datetime).single());
    }
    None
}

pub fn now_fixed_offset() -> DateTime<FixedOffset> {
    Utc::now().fixed_offset()
}

pub fn normalize_tenant_suffix(tenant_id: &str) -> String {
    tenant_id
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

pub fn apply_tenant_to_table(
    table: &str,
    isolation: TenantIsolationLevel,
    tenant_id: &str,
) -> String {
    match isolation {
        TenantIsolationLevel::SharedRow => table.to_string(),
        TenantIsolationLevel::SeparateTable => {
            format!("{table}_{}", normalize_tenant_suffix(tenant_id))
        }
        TenantIsolationLevel::SeparateSchema | TenantIsolationLevel::SeparateDatabase => {
            table.to_string()
        }
    }
}
