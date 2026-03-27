use crate::config::TenantIsolationLevel;

use super::{ShardingAlgorithm, ShardingValue, apply_tenant_to_table};

#[derive(Debug, Clone, Default)]
pub struct TenantShardingAlgorithm;

impl TenantShardingAlgorithm {
    pub fn shard_for(
        &self,
        table: &str,
        tenant_id: &str,
        isolation: TenantIsolationLevel,
    ) -> String {
        apply_tenant_to_table(table, isolation, tenant_id)
    }
}

impl ShardingAlgorithm for TenantShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        let tenant_id = sharding_value.as_str().unwrap_or_default();
        available_targets
            .iter()
            .find(|target| {
                target.ends_with(crate::algorithm::normalize_tenant_suffix(tenant_id).as_str())
            })
            .cloned()
            .map(|value| vec![value])
            .unwrap_or_else(|| available_targets.to_vec())
    }

    fn do_range_sharding(
        &self,
        available_targets: &[String],
        _lower: &ShardingValue,
        _upper: &ShardingValue,
    ) -> Vec<String> {
        available_targets.to_vec()
    }

    fn algorithm_type(&self) -> &str {
        "tenant"
    }
}
