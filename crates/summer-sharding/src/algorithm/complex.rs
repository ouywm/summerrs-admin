use crate::{
    algorithm::{TimeRangeShardingAlgorithm, now_fixed_offset},
    config::{TableRuleConfig, TenantIsolationLevel},
    error::Result,
};

use super::{ShardingAlgorithm, ShardingValue, apply_tenant_to_table};

#[derive(Debug, Clone)]
pub struct ComplexShardingAlgorithm {
    time: Option<TimeRangeShardingAlgorithm>,
}

impl ComplexShardingAlgorithm {
    pub fn from_rule(rule: &TableRuleConfig) -> Result<Self> {
        let time = if rule.algorithm_props.contains_key("granularity")
            || rule
                .actual_tables
                .pattern()
                .is_some_and(|value| value.contains("${yyyy"))
        {
            Some(TimeRangeShardingAlgorithm::from_rule(rule)?)
        } else {
            None
        };
        Ok(Self { time })
    }

    pub fn shard_for_tenant_and_time(
        &self,
        table_pattern: &str,
        tenant_id: &str,
        isolation: TenantIsolationLevel,
        sharding_value: &ShardingValue,
    ) -> String {
        let base = self
            .time
            .as_ref()
            .and_then(|algorithm| {
                sharding_value
                    .as_datetime()
                    .map(|dt| algorithm.render_target(table_pattern, dt))
            })
            .unwrap_or_else(|| table_pattern.to_string());
        apply_tenant_to_table(base.as_str(), isolation, tenant_id)
    }
}

impl ShardingAlgorithm for ComplexShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        if let Some(algorithm) = &self.time {
            return algorithm.do_sharding(available_targets, sharding_value);
        }
        available_targets.to_vec()
    }

    fn do_range_sharding(
        &self,
        available_targets: &[String],
        lower: &ShardingValue,
        upper: &ShardingValue,
    ) -> Vec<String> {
        if let Some(algorithm) = &self.time {
            return algorithm.do_range_sharding(available_targets, lower, upper);
        }
        if let Some(pattern) = available_targets
            .first()
            .filter(|value| value.contains("${yyyy"))
        {
            return self
                .time
                .clone()
                .unwrap_or(TimeRangeShardingAlgorithm {
                    granularity: crate::algorithm::TimeGranularity::Month,
                    pre_create_periods: 0,
                    retention_periods: 12,
                })
                .candidate_targets(pattern, now_fixed_offset());
        }
        available_targets.to_vec()
    }

    fn algorithm_type(&self) -> &str {
        "complex"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{FixedOffset, TimeZone};
    use serde_json::json;

    use crate::{
        algorithm::{ComplexShardingAlgorithm, ShardingAlgorithm, ShardingValue},
        config::{ActualTablesConfig, TableRuleConfig, TenantIsolationLevel},
    };

    fn time_rule() -> TableRuleConfig {
        TableRuleConfig {
            logic_table: "ai.log".to_string(),
            actual_tables: ActualTablesConfig::Pattern("ai.log_${yyyyMM}".to_string()),
            sharding_column: "create_time".to_string(),
            algorithm: "complex".to_string(),
            algorithm_props: BTreeMap::from([("granularity".to_string(), json!("month"))]),
            key_generator: None,
        }
    }

    #[test]
    fn complex_algorithm_applies_time_and_tenant_suffix() {
        let algorithm = ComplexShardingAlgorithm::from_rule(&time_rule()).expect("algorithm");
        let datetime = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
            .unwrap();

        let target = algorithm.shard_for_tenant_and_time(
            "ai.log_${yyyyMM}",
            "T-ENT-01",
            TenantIsolationLevel::SeparateTable,
            &ShardingValue::DateTime(datetime),
        );

        assert_eq!(target, "ai.log_202603_tent01");
    }

    #[test]
    fn complex_algorithm_delegates_exact_time_sharding() {
        let algorithm = ComplexShardingAlgorithm::from_rule(&time_rule()).expect("algorithm");
        let datetime = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 3, 1, 0, 0, 0)
            .unwrap();
        let available = vec!["ai.log_202602".to_string(), "ai.log_202603".to_string()];

        let routed = algorithm.do_sharding(&available, &ShardingValue::DateTime(datetime));

        assert_eq!(routed, vec!["ai.log_202603".to_string()]);
    }
}
