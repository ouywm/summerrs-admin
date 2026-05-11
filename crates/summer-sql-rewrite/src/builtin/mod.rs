pub mod auto_fill;
pub mod data_scope;
pub mod optimistic_lock;
pub mod probe;

pub use auto_fill::{AutoFillConfig, AutoFillPlugin, CurrentUser};
pub use data_scope::{DataScope, DataScopeConfig, DataScopePlugin};
pub use optimistic_lock::{OptimisticLockConfig, OptimisticLockPlugin, OptimisticLockValue};
pub use probe::ProbePlugin;

pub(crate) fn entity_to_qualified(entity: &impl sea_orm::EntityName) -> crate::QualifiedTableName {
    crate::QualifiedTableName {
        schema: entity.schema_name().map(|s| s.to_string()),
        table: entity.table_name().to_string(),
    }
}

/// 判断 `pattern` 是否匹配 `candidate`。
/// - table 名大小写不敏感
/// - pattern 没有 schema 时只比较 table 名（宽松匹配）
/// - pattern 有 schema 时 schema 也必须匹配
pub(crate) fn matches_name(
    pattern: &crate::QualifiedTableName,
    candidate: &crate::QualifiedTableName,
) -> bool {
    if !pattern.table.eq_ignore_ascii_case(&candidate.table) {
        return false;
    }
    match (&pattern.schema, &candidate.schema) {
        (Some(ps), Some(cs)) => ps.eq_ignore_ascii_case(cs),
        (None, _) => true,
        (Some(_), None) => false,
    }
}
