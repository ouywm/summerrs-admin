use serde::{Deserialize, Serialize};

const fn default_cdc_batch_size() -> usize {
    5_000
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
