use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use summer::config::Configurable;

use super::{
    BindingGroupConfig, DEFAULT_BOOTSTRAP_DATASOURCE, ReadWriteSplittingConfig,
    ShardingSectionConfig, TableRuleConfig, split_qualified_name,
};
use crate::{
    config::{TenantConfig, TenantIsolationLevel},
    error::{Result, ShardingError},
};

// ---------------------------------------------------------------------------
// 审计配置（still used by connector/connection/audit.rs for slow_query / fanout metrics）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_slow_query_threshold")]
    pub slow_query_threshold_ms: u64,
    #[serde(default)]
    pub log_full_scatter: bool,
    #[serde(default)]
    pub log_no_sharding_key: bool,
}

fn default_slow_query_threshold() -> u64 {
    1000
}

// ---------------------------------------------------------------------------
// 运行时分片配置
// ---------------------------------------------------------------------------

/// 运行时分片配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShardingConfig {
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

impl ShardingConfig {
    pub fn validate(&self) -> Result<()> {
        let mut seen_logic_tables = BTreeSet::new();
        for rule in &self.sharding.tables {
            if rule.logic_table.trim().is_empty() {
                return Err(ShardingError::Config(
                    "table rule logic_table cannot be empty".to_string(),
                ));
            }
            if !seen_logic_tables.insert(rule.logic_table.to_lowercase()) {
                return Err(ShardingError::Config(format!(
                    "duplicate table rule for `{}`",
                    rule.logic_table
                )));
            }
            if rule.sharding_column.trim().is_empty() {
                return Err(ShardingError::Config(format!(
                    "table rule `{}` sharding_column cannot be empty",
                    rule.logic_table
                )));
            }
            if rule.algorithm.trim().is_empty() {
                return Err(ShardingError::Config(format!(
                    "table rule `{}` algorithm cannot be empty",
                    rule.logic_table
                )));
            }
        }

        if self.read_write_splitting.enabled {
            for rule in &self.read_write_splitting.rules {
                if rule.primary.trim().is_empty() {
                    return Err(ShardingError::Config(format!(
                        "read/write rule `{}` primary cannot be empty",
                        rule.name
                    )));
                }
            }
        }

        for group in &self.sharding.binding_groups {
            if group.tables.len() < 2 {
                return Err(ShardingError::Config(
                    "binding group requires at least two tables".to_string(),
                ));
            }
            if group.sharding_column.trim().is_empty() {
                return Err(ShardingError::Config(
                    "binding group sharding_column cannot be empty".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn table_rule(&self, logic_table: &str) -> Option<&TableRuleConfig> {
        self.sharding
            .tables
            .iter()
            .find(|rule| rule.matches_logic_table(logic_table))
    }

    pub fn is_broadcast_table(&self, logic_table: &str) -> bool {
        let (_, table_only) = split_qualified_name(logic_table);
        self.sharding
            .global
            .broadcast_tables
            .iter()
            .any(|candidate| {
                candidate.eq_ignore_ascii_case(logic_table)
                    || split_qualified_name(candidate.as_str())
                        .1
                        .eq_ignore_ascii_case(table_only)
            })
    }

    pub fn is_tenant_shared_table(&self, logic_table: &str) -> bool {
        let (_, table_only) = split_qualified_name(logic_table);
        self.tenant.shared_tables.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(logic_table)
                || split_qualified_name(candidate.as_str())
                    .1
                    .eq_ignore_ascii_case(table_only)
        })
    }

    pub fn binding_group_for(&self, logic_table: &str) -> Option<&BindingGroupConfig> {
        let (_, table_only) = split_qualified_name(logic_table);
        self.sharding.binding_groups.iter().find(|group| {
            group.tables.iter().any(|table| {
                table.eq_ignore_ascii_case(logic_table)
                    || split_qualified_name(table.as_str())
                        .1
                        .eq_ignore_ascii_case(table_only)
            })
        })
    }

    pub fn default_datasource_name(&self) -> Option<&str> {
        Some(
            self.sharding
                .global
                .default_datasource
                .as_deref()
                .unwrap_or(DEFAULT_BOOTSTRAP_DATASOURCE),
        )
    }

    pub fn schema_primary_datasource(&self, _schema: &str) -> Option<&str> {
        self.default_datasource_name()
    }

    pub fn default_tenant_isolation(&self) -> TenantIsolationLevel {
        self.tenant.default_isolation
    }
}

/// Summer 框架启动期分片配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Configurable)]
#[config_prefix = "summer-sharding"]
pub struct SummerShardingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

impl SummerShardingConfig {
    pub fn into_runtime_config(self) -> Result<ShardingConfig> {
        let config = ShardingConfig {
            tenant: self.tenant,
            sharding: self.sharding,
            read_write_splitting: self.read_write_splitting,
            audit: self.audit,
        };
        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
impl ShardingConfig {
    pub fn from_test_str(input: &str) -> Result<Self> {
        let wrapped = wrap_test_config(input);
        let path = write_temp_config(wrapped.as_str())?;
        let registry = summer::config::toml::TomlConfigRegistry::new(
            path.as_path(),
            summer::config::env::Env::from_string("dev"),
        )
        .map_err(|err| ShardingError::Config(err.to_string()))?;
        let config = summer::config::ConfigRegistry::get_config::<SummerShardingConfig>(&registry)
            .map_err(|err| ShardingError::Config(err.to_string()))?;
        let runtime = config.into_runtime_config()?;
        let _ = std::fs::remove_file(&path);
        Ok(runtime)
    }
}

#[cfg(test)]
fn wrap_test_config(input: &str) -> String {
    let mut output = String::from("[summer-sharding]\n");
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            output.push_str(&line.replacen("[[", "[[summer-sharding.", 1));
            output.push('\n');
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            output.push_str(&line.replacen('[', "[summer-sharding.", 1));
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

#[cfg(test)]
fn write_temp_config(content: &str) -> Result<std::path::PathBuf> {
    static TEST_CONFIG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    let path = std::env::temp_dir().join(format!(
        "summer-sharding-test-{}-{}.toml",
        std::process::id(),
        TEST_CONFIG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_BOOTSTRAP_DATASOURCE, ShardingConfig};

    #[test]
    fn config_expands_env_and_matches_short_logic_table() {
        let config = ShardingConfig::from_test_str(
            r#"
            [[sharding.tables]]
            logic_table = "ai.log"
            actual_tables = "ai.log_${yyyyMM}"
            sharding_column = "create_time"
            algorithm = "time_range"
            "#,
        )
        .expect("config should parse");

        assert!(config.table_rule("log").is_some());
        assert_eq!(
            config.schema_primary_datasource("ai"),
            Some(DEFAULT_BOOTSTRAP_DATASOURCE)
        );
    }

    #[test]
    fn config_defaults_bootstrap_datasource_name() {
        let config = ShardingConfig::from_test_str(
            r#"
            [tenant]
            enabled = true
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            config.default_datasource_name(),
            Some(DEFAULT_BOOTSTRAP_DATASOURCE)
        );
    }
}
