use serde::{Deserialize, Serialize};

use super::{ConfigProps, split_qualified_name};
use crate::config::ReadWriteRuleConfig;

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
