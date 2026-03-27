use std::sync::Arc;

use regex::Regex;

use crate::{
    algorithm::TimeRangeShardingAlgorithm,
    config::{ActualTablesConfig, ShardingConfig, TableRuleConfig},
    connector::statement::StatementContext,
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub struct TableRouter {
    _config: Arc<ShardingConfig>,
}

impl TableRouter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self { _config: config }
    }

    pub fn available_targets(
        &self,
        rule: &TableRuleConfig,
        analysis: &StatementContext,
    ) -> Result<Vec<String>> {
        match rule.algorithm.as_str() {
            "time_range" => {
                let pattern = rule
                    .actual_tables
                    .pattern()
                    .ok_or_else(|| {
                        ShardingError::Config(format!(
                            "time_range rule `{}` requires a pattern actual_tables",
                            rule.logic_table
                        ))
                    })?
                    .to_string();
                if analysis
                    .sharding_condition(rule.sharding_column.as_str())
                    .is_some()
                    || !analysis
                        .insert_values(rule.sharding_column.as_str())
                        .is_empty()
                {
                    Ok(vec![pattern])
                } else {
                    self.expand_all_targets(rule, crate::algorithm::now_fixed_offset())
                }
            }
            _ => self.expand_all_targets(rule, crate::algorithm::now_fixed_offset()),
        }
    }

    pub fn expand_all_targets(
        &self,
        rule: &TableRuleConfig,
        now: chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<Vec<String>> {
        match &rule.actual_tables {
            ActualTablesConfig::Explicit(values) => Ok(values.clone()),
            ActualTablesConfig::Pattern(pattern) => {
                if pattern.contains("${yyyy") {
                    let algorithm = TimeRangeShardingAlgorithm::from_rule(rule)?;
                    Ok(algorithm.candidate_targets(pattern.as_str(), now))
                } else if let Some(expanded) = expand_numeric_pattern(pattern.as_str())? {
                    Ok(expanded)
                } else {
                    Ok(vec![pattern.clone()])
                }
            }
        }
    }
}

fn expand_numeric_pattern(pattern: &str) -> Result<Option<Vec<String>>> {
    let regex = Regex::new(r"\$\{(\d+)\.\.(\d+)\}")
        .map_err(|err| ShardingError::Config(err.to_string()))?;
    let Some(captures) = regex.captures(pattern) else {
        return Ok(None);
    };
    let start = captures
        .get(1)
        .and_then(|value| value.as_str().parse::<u32>().ok())
        .ok_or_else(|| ShardingError::Config("invalid numeric range pattern".to_string()))?;
    let end = captures
        .get(2)
        .and_then(|value| value.as_str().parse::<u32>().ok())
        .ok_or_else(|| ShardingError::Config("invalid numeric range pattern".to_string()))?;
    if start > end {
        return Err(ShardingError::Config(
            "numeric range pattern start must be less than or equal to end".to_string(),
        ));
    }
    let matched = captures
        .get(0)
        .ok_or_else(|| ShardingError::Config("invalid numeric range placeholder".to_string()))?
        .as_str()
        .to_string();
    Ok(Some(
        (start..=end)
            .map(|value| pattern.replace(matched.as_str(), value.to_string().as_str()))
            .collect(),
    ))
}
