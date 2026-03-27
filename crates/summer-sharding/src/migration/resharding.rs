#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReshardingMove {
    pub value: i64,
    pub from_shard: usize,
    pub to_shard: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ReshardingPlanner;

impl ReshardingPlanner {
    pub fn plan_hash_mod_expand(
        &self,
        old_shard_count: usize,
        new_shard_count: usize,
        samples: impl IntoIterator<Item = i64>,
    ) -> Vec<ReshardingMove> {
        samples
            .into_iter()
            .filter_map(|value| {
                let from = value.rem_euclid(old_shard_count as i64) as usize;
                let to = value.rem_euclid(new_shard_count as i64) as usize;
                (from != to).then_some(ReshardingMove {
                    value,
                    from_shard: from,
                    to_shard: to,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::migration::ReshardingPlanner;

    #[test]
    fn resharding_planner_detects_moved_hash_slots() {
        let planner = ReshardingPlanner;
        let moves = planner.plan_hash_mod_expand(2, 4, [1, 2, 3, 4]);
        assert!(!moves.is_empty());
    }
}
