#[derive(Debug, Clone, Default)]
pub struct DdlScheduler;

impl DdlScheduler {
    pub fn schedule(&self, shards: &[String], statements: &[String]) -> Vec<(String, Vec<String>)> {
        shards
            .iter()
            .cloned()
            .map(|shard| (shard, statements.to_vec()))
            .collect()
    }

    pub fn schedule_batches(&self, shards: &[String], concurrency: usize) -> Vec<Vec<String>> {
        let concurrency = concurrency.max(1);
        shards
            .chunks(concurrency)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::ddl::DdlScheduler;

    #[test]
    fn ddl_scheduler_groups_shards_by_concurrency() {
        let scheduler = DdlScheduler;
        let batches = scheduler.schedule_batches(
            &[
                "log_0".to_string(),
                "log_1".to_string(),
                "log_2".to_string(),
            ],
            2,
        );
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], vec!["log_0".to_string(), "log_1".to_string()]);
    }
}
