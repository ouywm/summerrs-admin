use regex::Regex;

use crate::{
    config::TableRuleConfig,
    error::{Result, ShardingError},
};

use super::{ShardingAlgorithm, ShardingValue};

#[derive(Debug, Clone)]
pub struct InlineShardingAlgorithm {
    expression: String,
}

impl InlineShardingAlgorithm {
    pub fn from_rule(rule: &TableRuleConfig) -> Result<Self> {
        let expression = rule
            .algorithm_props
            .get("expression")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                ShardingError::Config(format!(
                    "inline rule `{}` requires algorithm_props.expression",
                    rule.logic_table
                ))
            })?;
        Ok(Self {
            expression: expression.to_string(),
        })
    }

    fn render(&self, value: &ShardingValue) -> Option<String> {
        let raw = match value {
            ShardingValue::Int(number) => number.to_string(),
            ShardingValue::Str(text) => text.clone(),
            ShardingValue::DateTime(datetime) => datetime.timestamp_millis().to_string(),
            ShardingValue::Null => return None,
        };
        let mut rendered = self.expression.replace("${value}", raw.as_str());

        let regex = Regex::new(r"\$\{value\s*%\s*(\d+)\}").ok()?;
        for captures in regex.captures_iter(self.expression.as_str()) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            let modulus = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<i64>().ok())
                .unwrap_or(1);
            let base = value.as_i64().unwrap_or_default();
            rendered = rendered.replace(
                matched.as_str(),
                (base.rem_euclid(modulus)).to_string().as_str(),
            );
        }

        Some(rendered)
    }
}

impl ShardingAlgorithm for InlineShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        let Some(target) = self.render(sharding_value) else {
            return available_targets.to_vec();
        };
        if available_targets.is_empty() {
            return vec![target];
        }
        available_targets
            .iter()
            .find(|candidate| candidate.as_str() == target)
            .cloned()
            .map(|candidate| vec![candidate])
            .unwrap_or_else(|| vec![target])
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
        "inline"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::{
        algorithm::{InlineShardingAlgorithm, ShardingAlgorithm, ShardingValue},
        config::{ActualTablesConfig, TableRuleConfig},
        error::ShardingError,
    };

    fn rule(expression: Option<&str>) -> TableRuleConfig {
        let mut algorithm_props = BTreeMap::new();
        if let Some(expression) = expression {
            algorithm_props.insert("expression".to_string(), json!(expression));
        }
        TableRuleConfig {
            logic_table: "ai.log".to_string(),
            actual_tables: ActualTablesConfig::Explicit(vec![
                "ai.log_0".to_string(),
                "ai.log_1".to_string(),
            ]),
            sharding_column: "tenant_id".to_string(),
            algorithm: "inline".to_string(),
            algorithm_props,
            key_generator: None,
        }
    }

    #[test]
    fn inline_requires_expression() {
        let error = InlineShardingAlgorithm::from_rule(&rule(None)).expect_err("config error");
        assert!(matches!(error, ShardingError::Config(_)));
    }

    #[test]
    fn inline_renders_mod_expression() {
        let algorithm =
            InlineShardingAlgorithm::from_rule(&rule(Some("ai.log_${value % 2}"))).expect("rule");
        let targets = vec!["ai.log_0".to_string(), "ai.log_1".to_string()];

        let routed = algorithm.do_sharding(&targets, &ShardingValue::Int(7));

        assert_eq!(routed, vec!["ai.log_1".to_string()]);
    }
}
