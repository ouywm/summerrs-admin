use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use summer::config::Configurable;

use crate::error::{Result, ShardingError};

use super::{
    DataSourceConfig, DataSourceRole, ReadWriteRuleConfig, TenantConfig, TenantIsolationLevel,
};

pub type ConfigProps = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActualTablesConfig {
    Pattern(String),
    Explicit(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BindingGroupConfig {
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub sharding_column: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KeyGeneratorConfig {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub props: ConfigProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRuleConfig {
    pub logic_table: String,
    pub actual_tables: ActualTablesConfig,
    pub sharding_column: String,
    pub algorithm: String,
    #[serde(default)]
    pub algorithm_props: ConfigProps,
    #[serde(default)]
    pub key_generator: Option<KeyGeneratorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptRuleConfig {
    pub table: String,
    pub column: String,
    pub cipher_column: String,
    #[serde(default)]
    pub assisted_query_column: Option<String>,
    #[serde(default)]
    pub algorithm: String,
    #[serde(default)]
    pub key_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<EncryptRuleConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LookupIndexConfig {
    pub logic_table: String,
    pub lookup_column: String,
    pub lookup_table: String,
    pub sharding_column: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingRuleConfig {
    pub table: String,
    pub column: String,
    pub algorithm: String,
    #[serde(default)]
    pub show_first: usize,
    #[serde(default)]
    pub show_last: usize,
    #[serde(default = "default_mask_char")]
    pub mask_char: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<MaskingRuleConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShadowConditionKind {
    #[default]
    Header,
    Column,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowConditionConfig {
    #[serde(rename = "type", default)]
    pub kind: ShadowConditionKind,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowTableModeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowDatabaseModeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub datasource: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_shadow_suffix")]
    pub shadow_suffix: String,
    #[serde(default)]
    pub table_mode: ShadowTableModeConfig,
    #[serde(default)]
    pub database_mode: ShadowDatabaseModeConfig,
    #[serde(default)]
    pub conditions: Vec<ShadowConditionConfig>,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            shadow_suffix: default_shadow_suffix(),
            table_mode: ShadowTableModeConfig::default(),
            database_mode: ShadowDatabaseModeConfig::default(),
            conditions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineDdlConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_online_ddl_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_online_ddl_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_online_ddl_cutover_lock_timeout_ms")]
    pub cutover_lock_timeout_ms: u64,
    #[serde(default = "default_online_ddl_cleanup_delay_hours")]
    pub cleanup_delay_hours: u64,
}

impl Default for OnlineDdlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            concurrency: default_online_ddl_concurrency(),
            batch_size: default_online_ddl_batch_size(),
            cutover_lock_timeout_ms: default_online_ddl_cutover_lock_timeout_ms(),
            cleanup_delay_hours: default_online_ddl_cleanup_delay_hours(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdcTaskConfig {
    pub name: String,
    #[serde(default)]
    pub source_tables: Vec<String>,
    #[serde(default)]
    pub sink_tables: Vec<String>,
    #[serde(default)]
    pub transformer: Option<String>,
    #[serde(default = "default_cdc_batch_size")]
    pub batch_size: usize,
    #[serde(default)]
    pub source_filter: Option<String>,
    #[serde(default)]
    pub sink_schema: Option<String>,
    #[serde(default)]
    pub sink_type: Option<String>,
    #[serde(default)]
    pub sink_uri: Option<String>,
    #[serde(default)]
    pub delete_after_migrate: bool,
}

impl Default for CdcTaskConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            source_tables: Vec::new(),
            sink_tables: Vec::new(),
            transformer: None,
            batch_size: default_cdc_batch_size(),
            source_filter: None,
            sink_schema: None,
            sink_type: None,
            sink_uri: None,
            delete_after_migrate: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CdcConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tasks: Vec<CdcTaskConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
    #[serde(default)]
    pub log_full_scatter: bool,
    #[serde(default)]
    pub log_no_sharding_key: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            slow_query_threshold_ms: default_slow_query_threshold_ms(),
            log_full_scatter: false,
            log_no_sharding_key: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShardingGlobalConfig {
    #[serde(default)]
    pub broadcast_tables: Vec<String>,
    #[serde(default)]
    pub default_datasource: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShardingSectionConfig {
    #[serde(default)]
    pub tables: Vec<TableRuleConfig>,
    #[serde(default)]
    pub binding_groups: Vec<BindingGroupConfig>,
    #[serde(default)]
    pub lookup_indexes: Vec<LookupIndexConfig>,
    #[serde(default)]
    pub global: ShardingGlobalConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReadWriteSplittingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ReadWriteRuleConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShardingConfig {
    #[serde(default)]
    pub datasources: BTreeMap<String, DataSourceConfig>,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    #[serde(default)]
    pub encrypt: EncryptConfig,
    #[serde(default)]
    pub masking: MaskingConfig,
    #[serde(default)]
    pub shadow: ShadowConfig,
    #[serde(default)]
    pub online_ddl: OnlineDdlConfig,
    #[serde(default)]
    pub cdc: CdcConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Configurable)]
#[config_prefix = "summer-sharding"]
pub struct SummerShardingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub datasources: BTreeMap<String, DataSourceConfig>,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub sharding: ShardingSectionConfig,
    #[serde(default)]
    pub read_write_splitting: ReadWriteSplittingConfig,
    #[serde(default)]
    pub encrypt: EncryptConfig,
    #[serde(default)]
    pub masking: MaskingConfig,
    #[serde(default)]
    pub shadow: ShadowConfig,
    #[serde(default)]
    pub online_ddl: OnlineDdlConfig,
    #[serde(default)]
    pub cdc: CdcConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

impl SummerShardingConfig {
    pub fn into_runtime_config(self) -> Result<ShardingConfig> {
        let config = ShardingConfig {
            datasources: self.datasources,
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

impl ActualTablesConfig {
    pub fn entries(&self) -> Vec<String> {
        match self {
            Self::Pattern(pattern) => vec![pattern.clone()],
            Self::Explicit(values) => values.clone(),
        }
    }

    pub fn pattern(&self) -> Option<&str> {
        match self {
            Self::Pattern(pattern) => Some(pattern.as_str()),
            Self::Explicit(_) => None,
        }
    }
}

impl TableRuleConfig {
    pub fn logic_table_parts(&self) -> (Option<&str>, &str) {
        split_qualified_name(self.logic_table.as_str())
    }

    pub fn matches_logic_table(&self, value: &str) -> bool {
        if self.logic_table.eq_ignore_ascii_case(value) {
            return true;
        }
        let (_, table) = split_qualified_name(value);
        let (_, logic_table) = self.logic_table_parts();
        logic_table.eq_ignore_ascii_case(table)
    }
}

impl ShardingConfig {
    pub fn validate(&self) -> Result<()> {
        if self.datasources.is_empty() {
            return Err(ShardingError::Config(
                "at least one datasource must be configured".to_string(),
            ));
        }

        if let Some(default_datasource) = self.sharding.global.default_datasource.as_deref()
            && !self.datasources.contains_key(default_datasource)
        {
            return Err(ShardingError::Config(format!(
                "default datasource `{default_datasource}` is not defined"
            )));
        }

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
                if !self.datasources.contains_key(rule.primary.as_str()) {
                    return Err(ShardingError::Config(format!(
                        "read/write rule `{}` references missing primary `{}`",
                        rule.name, rule.primary
                    )));
                }
                for replica in &rule.replicas {
                    if !self.datasources.contains_key(replica.as_str()) {
                        return Err(ShardingError::Config(format!(
                            "read/write rule `{}` references missing replica `{}`",
                            rule.name, replica
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
        self.sharding
            .global
            .broadcast_tables
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(logic_table))
            || self
                .sharding
                .global
                .broadcast_tables
                .iter()
                .any(|candidate| {
                    split_qualified_name(candidate.as_str())
                        .1
                        .eq_ignore_ascii_case(split_qualified_name(logic_table).1)
                })
    }

    pub fn is_tenant_shared_table(&self, logic_table: &str) -> bool {
        self.tenant
            .shared_tables
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(logic_table))
            || self.tenant.shared_tables.iter().any(|candidate| {
                split_qualified_name(candidate.as_str())
                    .1
                    .eq_ignore_ascii_case(split_qualified_name(logic_table).1)
            })
    }

    pub fn binding_group_for(&self, logic_table: &str) -> Option<&BindingGroupConfig> {
        self.sharding.binding_groups.iter().find(|group| {
            group
                .tables
                .iter()
                .any(|table| table.eq_ignore_ascii_case(logic_table))
                || group.tables.iter().any(|table| {
                    split_qualified_name(table.as_str())
                        .1
                        .eq_ignore_ascii_case(split_qualified_name(logic_table).1)
                })
        })
    }

    pub fn lookup_indexes_for(&self, logic_table: &str) -> Vec<&LookupIndexConfig> {
        self.sharding
            .lookup_indexes
            .iter()
            .filter(|index| {
                index.logic_table.eq_ignore_ascii_case(logic_table)
                    || split_qualified_name(index.logic_table.as_str())
                        .1
                        .eq_ignore_ascii_case(split_qualified_name(logic_table).1)
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
        self.masking
            .rules
            .iter()
            .filter(|rule| {
                rule.table.eq_ignore_ascii_case(table)
                    || split_qualified_name(rule.table.as_str())
                        .1
                        .eq_ignore_ascii_case(split_qualified_name(table).1)
            })
            .collect()
    }

    pub fn shadow_routes_table(&self, table: &str) -> bool {
        self.shadow.table_mode.tables.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(table)
                || split_qualified_name(candidate.as_str())
                    .1
                    .eq_ignore_ascii_case(split_qualified_name(table).1)
        })
    }

    pub fn default_datasource_name(&self) -> Option<&str> {
        if let Some(default) = self.sharding.global.default_datasource.as_deref() {
            return Some(default);
        }
        self.datasources
            .iter()
            .find(|(_, config)| config.role == DataSourceRole::Primary)
            .map(|(name, _)| name.as_str())
            .or_else(|| self.datasources.keys().next().map(String::as_str))
    }

    pub fn schema_primary_datasource(&self, schema: &str) -> Option<&str> {
        self.datasources
            .iter()
            .find(|(_, config)| {
                config.role == DataSourceRole::Primary
                    && config
                        .schema
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case(schema))
            })
            .map(|(name, _)| name.as_str())
            .or_else(|| {
                self.datasources
                    .iter()
                    .find(|(_, config)| {
                        config
                            .schema
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case(schema))
                    })
                    .map(|(name, _)| name.as_str())
            })
    }

    pub fn default_tenant_isolation(&self) -> TenantIsolationLevel {
        self.tenant.default_isolation
    }
}

pub(crate) fn split_qualified_name(value: &str) -> (Option<&str>, &str) {
    match value.split_once('.') {
        Some((schema, table)) => (Some(schema), table),
        None => (None, value),
    }
}

const fn default_slow_query_threshold_ms() -> u64 {
    500
}

fn default_mask_char() -> String {
    "*".to_string()
}

fn default_shadow_suffix() -> String {
    "_shadow".to_string()
}

const fn default_online_ddl_concurrency() -> usize {
    3
}

const fn default_online_ddl_batch_size() -> usize {
    10_000
}

const fn default_online_ddl_cutover_lock_timeout_ms() -> u64 {
    5_000
}

const fn default_online_ddl_cleanup_delay_hours() -> u64 {
    24
}

const fn default_cdc_batch_size() -> usize {
    5_000
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
            let inner = trimmed.trim_start_matches("[[").trim_end_matches("]]");
            output.push_str(&line.replacen("[[", "[[summer-sharding.", 1));
            if inner.is_empty() {
                output.push('\n');
            } else {
                output.push('\n');
            }
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
    use super::ShardingConfig;

    #[test]
    fn config_expands_env_and_matches_short_logic_table() {
        let config = ShardingConfig::from_test_str(
            r#"
            [datasources.ds_default]
            uri = "${TEST_SHARDING_DATABASE_URL:postgres://localhost/app}"
            schema = "ai"
            role = "primary"

            [[sharding.tables]]
            logic_table = "ai.log"
            actual_tables = "ai.log_${yyyyMM}"
            sharding_column = "create_time"
            algorithm = "time_range"
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            config.datasources["ds_default"].uri,
            "postgres://localhost/app"
        );
        assert!(config.table_rule("log").is_some());
        assert_eq!(config.schema_primary_datasource("ai"), Some("ds_default"));
    }

    #[test]
    fn config_parses_lookup_masking_shadow_and_cdc_sections() {
        let config = ShardingConfig::from_test_str(
            r#"
            [datasources.ds_default]
            uri = "mock://db"
            schema = "ai"
            role = "primary"

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
              datasource = "ds_default"

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
    fn config_parses_datasource_pool_settings() {
        let config = ShardingConfig::from_test_str(
            r#"
            [datasources.ds_default]
            uri = "mock://db"
            schema = "ai"
            role = "primary"
            enable_logging = true
            min_connections = 4
            max_connections = 32
            connect_timeout = 1000
            idle_timeout = 2000
            acquire_timeout = 3000
            test_before_acquire = false
            "#,
        )
        .expect("config should parse");

        let datasource = &config.datasources["ds_default"];
        assert!(datasource.enable_logging);
        assert_eq!(datasource.min_connections, 4);
        assert_eq!(datasource.max_connections, 32);
        assert_eq!(datasource.connect_timeout, Some(1000));
        assert_eq!(datasource.idle_timeout, Some(2000));
        assert_eq!(datasource.acquire_timeout, Some(3000));
        assert!(!datasource.test_before_acquire);
    }
}
