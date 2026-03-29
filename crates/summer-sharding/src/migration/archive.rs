use chrono::{DateTime, FixedOffset};

use crate::{algorithm::TimeRangeShardingAlgorithm, config::TableRuleConfig, error::Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveCandidate {
    pub table: String,
    pub drop_sql: String,
}

#[derive(Debug, Clone, Default)]
pub struct ArchivePlanner;

impl ArchivePlanner {
    pub fn plan(
        &self,
        rule: &TableRuleConfig,
        now: DateTime<FixedOffset>,
    ) -> Result<Vec<ArchiveCandidate>> {
        let algorithm = TimeRangeShardingAlgorithm::from_rule(rule)?;
        let pattern = rule
            .actual_tables
            .pattern()
            .unwrap_or(rule.logic_table.as_str());
        let candidates =
            algorithm.history_targets(pattern, now, algorithm.retention_periods.saturating_add(12));
        let keep = algorithm.retention_periods;
        Ok(candidates
            .into_iter()
            .rev()
            .skip(keep)
            .map(|table| ArchiveCandidate {
                drop_sql: format!("DROP TABLE IF EXISTS {table}"),
                table,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::{config::ShardingConfig, migration::ArchivePlanner};

    #[test]
    fn archive_planner_returns_old_candidates() {
        let config = ShardingConfig::from_test_str(
            r#"
            [datasources.ds]
            uri = "mock://db"
            role = "primary"

            [[sharding.tables]]
            logic_table = "ai.log"
            actual_tables = "ai.log_${yyyyMM}"
            sharding_column = "create_time"
            algorithm = "time_range"

              [sharding.tables.algorithm_props]
              granularity = "month"
              retention_months = 1
            "#,
        )
        .expect("config");
        let planner = ArchivePlanner;
        let items = planner
            .plan(
                &config.sharding.tables[0],
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 3, 1, 0, 0, 0)
                    .unwrap(),
            )
            .expect("plan");
        assert!(!items.is_empty());
    }
}
