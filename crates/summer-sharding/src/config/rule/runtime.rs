use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use summer::config::Configurable;

use super::{
    AuditConfig, BindingGroupConfig, CdcConfig, DEFAULT_BOOTSTRAP_DATASOURCE, EncryptConfig,
    LookupIndexConfig, MaskingConfig, MaskingRuleConfig, OnlineDdlConfig, ReadWriteSplittingConfig,
    ShadowConfig, ShardingSectionConfig, TableRuleConfig, split_qualified_name,
};
use crate::{
    config::{TenantConfig, TenantIsolationLevel},
    error::{Result, ShardingError},
};

/// 运行时分片配置。
///
/// 这是经过启动配置归一化并验证后的内部配置对象，
/// 用于真正驱动路由、改写和执行流程。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShardingConfig {
    /// 多租户相关配置。
    #[serde(default)]
    pub tenant: TenantConfig,
    /// 分片规则配置。
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    /// 读写分离配置。
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    /// 加密配置。
    #[serde(default)]
    pub encrypt: EncryptConfig,
    /// 脱敏配置。
    #[serde(default)]
    pub masking: MaskingConfig,
    /// 影子流量配置。
    #[serde(default)]
    pub shadow: ShadowConfig,
    /// 在线 DDL 配置。
    #[serde(default)]
    pub online_ddl: OnlineDdlConfig,
    /// CDC 配置。
    #[serde(default)]
    pub cdc: CdcConfig,
    /// 审计配置。
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
                for replica in &rule.replicas {
                    if replica.trim().is_empty() {
                        return Err(ShardingError::Config(format!(
                            "read/write rule `{}` contains an empty replica name",
                            rule.name
                        )));
                    }
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

        if self.encrypt.enabled {
            for rule in &self.encrypt.rules {
                if rule.algorithm.trim().is_empty() || rule.key_env.trim().is_empty() {
                    return Err(ShardingError::Config(format!(
                        "encrypt rule `{}`.{} requires algorithm and key_env",
                        rule.table, rule.column
                    )));
                }
            }
        }

        if self.masking.enabled {
            for rule in &self.masking.rules {
                if rule.table.trim().is_empty()
                    || rule.column.trim().is_empty()
                    || rule.algorithm.trim().is_empty()
                {
                    return Err(ShardingError::Config(
                        "masking rules require table, column and algorithm".to_string(),
                    ));
                }
            }
        }

        for index in &self.sharding.lookup_indexes {
            if index.logic_table.trim().is_empty()
                || index.lookup_column.trim().is_empty()
                || index.lookup_table.trim().is_empty()
                || index.sharding_column.trim().is_empty()
            {
                return Err(ShardingError::Config(
                    "lookup index requires logic_table, lookup_column, lookup_table and sharding_column"
                        .to_string(),
                ));
            }
        }

        if self.shadow.enabled
            && self.shadow.database_mode.enabled
            && self.shadow.database_mode.datasource.is_none()
        {
            return Err(ShardingError::Config(
                "shadow database mode requires datasource".to_string(),
            ));
        }

        if self.online_ddl.enabled
            && (self.online_ddl.concurrency == 0 || self.online_ddl.batch_size == 0)
        {
            return Err(ShardingError::Config(
                "online_ddl concurrency and batch_size must be positive".to_string(),
            ));
        }

        if self.cdc.enabled {
            for task in &self.cdc.tasks {
                if task.name.trim().is_empty() {
                    return Err(ShardingError::Config(
                        "cdc task name cannot be empty".to_string(),
                    ));
                }
                if task.source_tables.is_empty() {
                    return Err(ShardingError::Config(format!(
                        "cdc task `{}` requires at least one source table",
                        task.name
                    )));
                }
                if task.batch_size == 0 {
                    return Err(ShardingError::Config(format!(
                        "cdc task `{}` batch_size must be positive",
                        task.name
                    )));
                }
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

    pub fn lookup_indexes_for(&self, logic_table: &str) -> Vec<&LookupIndexConfig> {
        let (_, table_only) = split_qualified_name(logic_table);
        self.sharding
            .lookup_indexes
            .iter()
            .filter(|index| {
                index.logic_table.eq_ignore_ascii_case(logic_table)
                    || split_qualified_name(index.logic_table.as_str())
                        .1
                        .eq_ignore_ascii_case(table_only)
            })
            .collect()
    }

    pub fn lookup_index_for(
        &self,
        logic_table: &str,
        lookup_column: &str,
    ) -> Option<&LookupIndexConfig> {
        self.lookup_indexes_for(logic_table)
            .into_iter()
            .find(|index| index.lookup_column.eq_ignore_ascii_case(lookup_column))
    }

    pub fn masking_rules_for(&self, table: &str) -> Vec<&MaskingRuleConfig> {
        let (_, table_only) = split_qualified_name(table);
        self.masking
            .rules
            .iter()
            .filter(|rule| {
                rule.table.eq_ignore_ascii_case(table)
                    || split_qualified_name(rule.table.as_str())
                        .1
                        .eq_ignore_ascii_case(table_only)
            })
            .collect()
    }

    pub fn shadow_routes_table(&self, table: &str) -> bool {
        let (_, table_only) = split_qualified_name(table);
        self.shadow.table_mode.tables.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(table)
                || split_qualified_name(candidate.as_str())
                    .1
                    .eq_ignore_ascii_case(table_only)
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
///
/// 该结构体负责从配置中心或 TOML 中反序列化，
/// 随后通过 `into_runtime_config()` 转成运行时使用的 `ShardingConfig`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Configurable)]
#[config_prefix = "summer-sharding"]
pub struct SummerShardingConfig {
    /// 是否启用分片插件。
    #[serde(default)]
    pub enabled: bool,
    /// 多租户相关配置。
    #[serde(default)]
    pub tenant: TenantConfig,
    /// 分片规则配置。
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    /// 读写分离配置。
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    /// 加密配置。
    #[serde(default)]
    pub encrypt: EncryptConfig,
    /// 脱敏配置。
    #[serde(default)]
    pub masking: MaskingConfig,
    /// 影子流量配置。
    #[serde(default)]
    pub shadow: ShadowConfig,
    /// 在线 DDL 配置。
    #[serde(default)]
    pub online_ddl: OnlineDdlConfig,
    /// CDC 配置。
    #[serde(default)]
    pub cdc: CdcConfig,
    /// 审计配置。
    #[serde(default)]
    pub audit: AuditConfig,
}

impl SummerShardingConfig {
    pub fn into_runtime_config(self) -> Result<ShardingConfig> {
        let config = ShardingConfig {
            tenant: self.tenant,
            sharding: self.sharding,
            read_write_splitting: self.read_write_splitting,
            encrypt: self.encrypt,
            masking: self.masking,
            shadow: self.shadow,
            online_ddl: self.online_ddl,
            cdc: self.cdc,
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
    fn config_parses_lookup_masking_shadow_and_cdc_sections() {
        let config = ShardingConfig::from_test_str(
            r#"
            [[sharding.tables]]
            logic_table = "ai.log"
            actual_tables = "ai.log_${yyyyMM}"
            sharding_column = "create_time"
            algorithm = "time_range"

            [[sharding.lookup_indexes]]
            logic_table = "ai.log"
            lookup_column = "trace_id"
            lookup_table = "ai.log_lookup_trace_id"
            sharding_column = "create_time"

            [masking]
            enabled = true

              [[masking.rules]]
              table = "ai.log"
              column = "client_ip"
              algorithm = "ip"

            [shadow]
            enabled = true
            shadow_suffix = "_shadow"

              [shadow.table_mode]
              enabled = true
              tables = ["ai.log"]

            [shadow.database_mode]
            enabled = true
            datasource = "shadow_dynamic"

              [[shadow.conditions]]
              type = "column"
              column = "is_shadow"
              value = "1"

            [online_ddl]
            enabled = true
            concurrency = 2
            batch_size = 2000

            [cdc]
            enabled = true

              [[cdc.tasks]]
              name = "expand_log"
              source_tables = ["ai.log_202603"]
              sink_tables = ["ai.log_0", "ai.log_1"]
              batch_size = 100
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            config
                .lookup_index_for("ai.log", "trace_id")
                .map(|item| item.lookup_table.as_str()),
            Some("ai.log_lookup_trace_id")
        );
        assert_eq!(config.masking_rules_for("log").len(), 1);
        assert!(config.shadow_routes_table("ai.log"));
        assert_eq!(config.online_ddl.batch_size, 2000);
        assert_eq!(config.cdc.tasks.len(), 1);
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
