mod audit;
mod cdc;
mod ddl;
mod encrypt;
mod lookup;
mod masking;
mod runtime;
mod shadow;
mod sharding;

use std::collections::BTreeMap;

pub use audit::AuditConfig;
pub use cdc::{CdcConfig, CdcTaskConfig};
pub use ddl::OnlineDdlConfig;
pub use encrypt::{EncryptConfig, EncryptRuleConfig};
pub use lookup::LookupIndexConfig;
pub use masking::{MaskingConfig, MaskingRuleConfig};
pub use runtime::{ShardingConfig, SummerShardingConfig};
pub use shadow::{
    ShadowConditionConfig, ShadowConditionKind, ShadowConfig, ShadowDatabaseModeConfig,
    ShadowTableModeConfig,
};
pub use sharding::{
    ActualTablesConfig, BindingGroupConfig, KeyGeneratorConfig, ReadWriteSplittingConfig,
    ShardingGlobalConfig, ShardingSectionConfig, TableRuleConfig,
};

/// 通用配置属性映射，用于承载算法或扩展能力的自定义参数。
pub type ConfigProps = BTreeMap<String, serde_json::Value>;

pub(crate) const DEFAULT_BOOTSTRAP_DATASOURCE: &str = "__bootstrap_primary";

pub(crate) fn split_qualified_name(value: &str) -> (Option<&str>, &str) {
    match value.split_once('.') {
        Some((schema, table)) => (Some(schema), table),
        None => (None, value),
    }
}
