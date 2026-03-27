use std::hash::{Hash, Hasher};

use crate::{
    config::TableRuleConfig,
    error::{Result, ShardingError},
};

use super::{ShardingAlgorithm, ShardingValue};

#[derive(Debug, Clone)]
pub struct HashModShardingAlgorithm {
    shard_count: usize,
}

impl HashModShardingAlgorithm {
    pub fn from_rule(rule: &TableRuleConfig) -> Result<Self> {
        let shard_count = rule
            .algorithm_props
            .get("count")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| {
                ShardingError::Config(format!(
                    "hash_mod rule `{}` requires integer algorithm_props.count",
                    rule.logic_table
                ))
            })? as usize;
        if shard_count == 0 {
            return Err(ShardingError::Config(
                "hash_mod shard count must be greater than zero".to_string(),
            ));
        }
        Ok(Self { shard_count })
    }

    fn shard_index(&self, value: &ShardingValue) -> usize {
        if let Some(number) = value.as_i64() {
            return number.rem_euclid(self.shard_count as i64) as usize;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.as_str().unwrap_or_default().hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count
    }
}

impl ShardingAlgorithm for HashModShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        let index = self.shard_index(sharding_value);
        available_targets
            .iter()
            .find(|target| target.ends_with(format!("_{index}").as_str()))
            .cloned()
            .map(|target| vec![target])
            .unwrap_or_else(|| {
                available_targets
                    .get(index)
                    .cloned()
                    .map(|target| vec![target])
                    .unwrap_or_default()
            })
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
        "hash_mod"
    }
}

#[cfg(test)]
mod tests {
    use super::HashModShardingAlgorithm;
    use crate::algorithm::{ShardingAlgorithm, ShardingValue};

    #[test]
    fn hash_mod_routes_to_suffix_target() {
        let algorithm = HashModShardingAlgorithm { shard_count: 4 };
        let targets = vec![
            "ai.token_0".to_string(),
            "ai.token_1".to_string(),
            "ai.token_2".to_string(),
            "ai.token_3".to_string(),
        ];

        let actual = algorithm.do_sharding(&targets, &ShardingValue::Int(7));
        assert_eq!(actual, vec!["ai.token_3".to_string()]);
    }
}
