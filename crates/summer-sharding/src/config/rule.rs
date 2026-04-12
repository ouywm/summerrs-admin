use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use summer::config::Configurable;

use crate::error::{Result, ShardingError};

use super::{ReadWriteRuleConfig, TenantConfig, TenantIsolationLevel};

const DEFAULT_BOOTSTRAP_DATASOURCE: &str = "__bootstrap_primary";

/// 通用配置属性映射，用于承载算法或扩展能力的自定义参数。
pub type ConfigProps = BTreeMap<String, serde_json::Value>;

/// 物理表集合配置。
///
/// 支持直接给出显式表名列表，或提供一个带占位符的模式字符串，
/// 由后续规则在运行时展开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActualTablesConfig {
    /// 物理表模式，例如 `ai.log_${yyyyMM}`。
    Pattern(String),
    /// 显式列出的物理表名列表。
    Explicit(Vec<String>),
}

/// 绑定表组配置。
///
/// 同一绑定组中的表应使用相同的分片键和路由结果，
/// 用于避免关联查询跨分片失配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BindingGroupConfig {
    /// 参与绑定的逻辑表列表。
    #[serde(default)]
    pub tables: Vec<String>,
    /// 绑定组共用的分片键列名。
    #[serde(default)]
    pub sharding_column: String,
}

/// 主键生成器配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KeyGeneratorConfig {
    /// 主键生成器类型，例如 `snowflake`、`tsid`。
    #[serde(rename = "type")]
    pub kind: String,
    /// 主键生成器的扩展参数。
    #[serde(flatten)]
    pub props: ConfigProps,
}

/// 逻辑表分片规则配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRuleConfig {
    /// 逻辑表名，支持带 schema 的限定名。
    pub logic_table: String,
    /// 物理表集合定义，可以是模式或显式列表。
    pub actual_tables: ActualTablesConfig,
    /// 路由该逻辑表时使用的分片键列。
    pub sharding_column: String,
    /// 分片算法类型。
    pub algorithm: String,
    /// 分片算法的扩展参数。
    #[serde(default)]
    pub algorithm_props: ConfigProps,
    /// 该表可选的主键生成器配置。
    #[serde(default)]
    pub key_generator: Option<KeyGeneratorConfig>,
}

/// 字段加密规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptRuleConfig {
    /// 需要加密的逻辑表。
    pub table: String,
    /// 明文字段名。
    pub column: String,
    /// 密文字段名。
    pub cipher_column: String,
    /// 辅助查询字段名，例如用于等值匹配或模糊检索。
    #[serde(default)]
    pub assisted_query_column: Option<String>,
    /// 加密算法名称。
    #[serde(default)]
    pub algorithm: String,
    /// 存放密钥的环境变量名。
    #[serde(default)]
    pub key_env: String,
}

/// 加密模块配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptConfig {
    /// 是否启用字段加密。
    #[serde(default)]
    pub enabled: bool,
    /// 加密规则列表。
    #[serde(default)]
    pub rules: Vec<EncryptRuleConfig>,
}

/// 查询列到分片列的 lookup 索引配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LookupIndexConfig {
    /// 被查询的逻辑表。
    pub logic_table: String,
    /// 用于 lookup 的查询列。
    pub lookup_column: String,
    /// 存储 lookup 关系的索引表。
    pub lookup_table: String,
    /// 真正参与分片计算的列。
    pub sharding_column: String,
}

/// 脱敏规则配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingRuleConfig {
    /// 需要脱敏的逻辑表。
    pub table: String,
    /// 需要脱敏的字段名。
    pub column: String,
    /// 脱敏算法名称。
    pub algorithm: String,
    /// 保留前缀字符数。
    #[serde(default)]
    pub show_first: usize,
    /// 保留后缀字符数。
    #[serde(default)]
    pub show_last: usize,
    /// 用于填充的脱敏字符。
    #[serde(default = "default_mask_char")]
    pub mask_char: String,
}

/// 脱敏模块配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingConfig {
    /// 是否启用脱敏。
    #[serde(default)]
    pub enabled: bool,
    /// 脱敏规则列表。
    #[serde(default)]
    pub rules: Vec<MaskingRuleConfig>,
}

/// 影子流量命中条件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShadowConditionKind {
    /// 从请求头判断是否进入影子链路。
    #[default]
    Header,
    /// 从 SQL 条件列判断是否进入影子链路。
    Column,
    /// 从 hint 判断是否进入影子链路。
    Hint,
}

/// 影子流量命中条件配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowConditionConfig {
    /// 条件类型。
    #[serde(rename = "type", default)]
    pub kind: ShadowConditionKind,
    /// 请求头键名或 hint 键名。
    #[serde(default)]
    pub key: Option<String>,
    /// SQL 条件列名。
    #[serde(default)]
    pub column: Option<String>,
    /// 命中条件的目标值。
    #[serde(default)]
    pub value: Option<String>,
}

/// 影子表模式配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowTableModeConfig {
    /// 是否启用影子表模式。
    #[serde(default)]
    pub enabled: bool,
    /// 需要路由到影子表的逻辑表列表。
    #[serde(default)]
    pub tables: Vec<String>,
}

/// 影子库模式配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShadowDatabaseModeConfig {
    /// 是否启用影子库模式。
    #[serde(default)]
    pub enabled: bool,
    /// 影子流量使用的数据源名称。
    #[serde(default)]
    pub datasource: Option<String>,
}

/// 影子流量总配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowConfig {
    /// 是否启用影子链路。
    #[serde(default)]
    pub enabled: bool,
    /// 影子表后缀。
    #[serde(default = "default_shadow_suffix")]
    pub shadow_suffix: String,
    /// 影子表模式配置。
    #[serde(default)]
    pub table_mode: ShadowTableModeConfig,
    /// 影子库模式配置。
    #[serde(default)]
    pub database_mode: ShadowDatabaseModeConfig,
    /// 影子命中条件列表。
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

/// 在线 DDL 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineDdlConfig {
    /// 是否启用在线 DDL。
    #[serde(default)]
    pub enabled: bool,
    /// 并发执行的分片任务数。
    #[serde(default = "default_online_ddl_concurrency")]
    pub concurrency: usize,
    /// 单批次处理的数据量。
    #[serde(default = "default_online_ddl_batch_size")]
    pub batch_size: usize,
    /// cutover 阶段获取锁的超时时间，单位毫秒。
    #[serde(default = "default_online_ddl_cutover_lock_timeout_ms")]
    pub cutover_lock_timeout_ms: u64,
    /// 清理旧表或临时资源的延迟时间，单位小时。
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

/// 单个 CDC 任务配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdcTaskConfig {
    /// 任务名称。
    pub name: String,
    /// 源表列表。
    #[serde(default)]
    pub source_tables: Vec<String>,
    /// 目标表列表。
    #[serde(default)]
    pub sink_tables: Vec<String>,
    /// 行转换器名称。
    #[serde(default)]
    pub transformer: Option<String>,
    /// 每批处理的记录数。
    #[serde(default = "default_cdc_batch_size")]
    pub batch_size: usize,
    /// 源端过滤表达式。
    #[serde(default)]
    pub source_filter: Option<String>,
    /// 目标端 schema。
    #[serde(default)]
    pub sink_schema: Option<String>,
    /// 目标 sink 类型。
    #[serde(default)]
    pub sink_type: Option<String>,
    /// 目标 sink 连接地址。
    #[serde(default)]
    pub sink_uri: Option<String>,
    /// 迁移完成后是否删除源数据。
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

/// CDC 总配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CdcConfig {
    /// 是否启用 CDC。
    #[serde(default)]
    pub enabled: bool,
    /// CDC 任务列表。
    #[serde(default)]
    pub tasks: Vec<CdcTaskConfig>,
}

/// 审计配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditConfig {
    /// 是否启用审计。
    #[serde(default)]
    pub enabled: bool,
    /// 慢查询阈值，单位毫秒。
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
    /// 是否记录全散射查询。
    #[serde(default)]
    pub log_full_scatter: bool,
    /// 是否记录缺失分片键的 SQL。
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

/// 分片全局配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShardingGlobalConfig {
    /// 广播表列表。
    #[serde(default)]
    pub broadcast_tables: Vec<String>,
    /// 默认数据源名称。
    #[serde(default)]
    pub default_datasource: Option<String>,
}

/// 分片规则主配置段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShardingSectionConfig {
    /// 表分片规则列表。
    #[serde(default)]
    pub tables: Vec<TableRuleConfig>,
    /// 绑定表组列表。
    #[serde(default)]
    pub binding_groups: Vec<BindingGroupConfig>,
    /// lookup 索引配置列表。
    #[serde(default)]
    pub lookup_indexes: Vec<LookupIndexConfig>,
    /// 全局分片配置。
    #[serde(default)]
    pub global: ShardingGlobalConfig,
}

/// 读写分离配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReadWriteSplittingConfig {
    /// 是否启用读写分离。
    #[serde(default)]
    pub enabled: bool,
    /// 读写分离规则列表。
    #[serde(default)]
    pub rules: Vec<ReadWriteRuleConfig>,
}

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
        Some(
            self.sharding
                .global
                .default_datasource
                .as_deref()
                .unwrap_or(DEFAULT_BOOTSTRAP_DATASOURCE),
        )
    }

    pub fn schema_primary_datasource(&self, schema: &str) -> Option<&str> {
        let _ = schema;
        self.default_datasource_name()
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
            Some(super::DEFAULT_BOOTSTRAP_DATASOURCE)
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
            Some(super::DEFAULT_BOOTSTRAP_DATASOURCE)
        );
    }
}
