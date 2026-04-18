use serde::{Deserialize, Serialize};

fn default_shadow_suffix() -> String {
    "_shadow".to_string()
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
