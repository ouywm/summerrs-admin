use chrono::{DateTime, FixedOffset};

use crate::{algorithm::TimeRangeShardingAlgorithm, config::TableRuleConfig, error::Result};

#[derive(Debug, Clone, Default)]
pub struct AutoTablePlanner;

impl AutoTablePlanner {
    pub fn plan_create_sql(
        &self,
        rule: &TableRuleConfig,
        base_table: &str,
        now: DateTime<FixedOffset>,
    ) -> Result<Vec<String>> {
        let algorithm = TimeRangeShardingAlgorithm::from_rule(rule)?;
        let pattern = rule
            .actual_tables
            .pattern()
            .unwrap_or(rule.logic_table.as_str());
        Ok(algorithm
            .candidate_targets(pattern, now)
            .into_iter()
            .map(|target| {
                format!("CREATE TABLE IF NOT EXISTS {target} (LIKE {base_table} INCLUDING ALL)")
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::{config::ShardingConfig, migration::AutoTablePlanner};

    #[test]
    fn auto_table_planner_generates_create_statements() {
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
              pre_create_months = 1
            "#,
        )
        .expect("config");
        let planner = AutoTablePlanner;
        let sql = planner
            .plan_create_sql(
                &config.sharding.tables[0],
                "ai.log_template",
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 3, 1, 0, 0, 0)
                    .unwrap(),
            )
            .expect("sql");
        assert!(!sql.is_empty());
        assert!(sql[0].contains("CREATE TABLE IF NOT EXISTS ai.log_"));
    }
}
