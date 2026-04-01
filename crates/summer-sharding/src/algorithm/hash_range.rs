use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use super::{ShardingAlgorithm, ShardingValue};

#[derive(Debug, Clone, Default)]
pub struct HashRangeShardingAlgorithm;

impl ShardingAlgorithm for HashRangeShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        if available_targets.is_empty() {
            return Vec::new();
        }

        let mut hasher = FxHasher::default();
        match sharding_value {
            ShardingValue::Int(value) => value.hash(&mut hasher),
            ShardingValue::Str(value) => value.hash(&mut hasher),
            ShardingValue::DateTime(value) => value.timestamp_millis().hash(&mut hasher),
            ShardingValue::Null => 0_u8.hash(&mut hasher),
        }
        vec![available_targets[(hasher.finish() as usize) % available_targets.len()].clone()]
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
        "hash_range"
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithm::{HashRangeShardingAlgorithm, ShardingAlgorithm, ShardingValue};

    #[test]
    fn hash_range_routes_deterministically() {
        let algorithm = HashRangeShardingAlgorithm;
        let targets = vec![
            "ds_0".to_string(),
            "ds_1".to_string(),
            "ds_2".to_string(),
            "ds_3".to_string(),
        ];

        let first = algorithm.do_sharding(&targets, &ShardingValue::Str("tenant-a".to_string()));
        let second = algorithm.do_sharding(&targets, &ShardingValue::Str("tenant-a".to_string()));

        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
    }

    #[test]
    fn hash_range_range_sharding_returns_all_targets() {
        let algorithm = HashRangeShardingAlgorithm;
        let targets = vec!["ds_0".to_string(), "ds_1".to_string()];

        let routed =
            algorithm.do_range_sharding(&targets, &ShardingValue::Int(1), &ShardingValue::Int(9));

        assert_eq!(routed, targets);
    }
}
