pub mod auto_fill;
pub mod data_scope;
pub mod optimistic_lock;
pub mod probe;

pub use auto_fill::{AutoFillConfig, AutoFillPlugin, CurrentUser};
pub use data_scope::{DataScope, DataScopeConfig, DataScopePlugin};
pub use optimistic_lock::{OptimisticLockConfig, OptimisticLockPlugin, OptimisticLockValue};
pub use probe::ProbePlugin;

pub(crate) fn entity_qualified_name<E: sea_orm::EntityName + Default>(entity: &E) -> String {
    match entity.schema_name() {
        Some(schema) => format!("{}.{}", schema, entity.table_name()),
        None => entity.table_name().to_string(),
    }
}
