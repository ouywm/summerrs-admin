use serde::{Deserialize, Serialize};

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
