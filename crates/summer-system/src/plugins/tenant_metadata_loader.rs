use std::sync::Arc;

use summer::component;
use summer_sharding::{
    SeaOrmTenantMetadataLoader, TenantIsolationLevel, TenantMetadataLoader, TenantMetadataRecord,
    TenantMetadataSchema,
};
use summer_system_model::entity::sys_tenant_datasource::{
    self, TenantDatasourceStatus as SystemTenantDatasourceStatus,
    TenantIsolationLevel as SystemTenantIsolationLevel,
};

#[derive(Debug, Clone, Copy, Default)]
struct SysTenantDatasourceSchema;

impl TenantMetadataSchema for SysTenantDatasourceSchema {
    type Entity = sys_tenant_datasource::Entity;

    fn into_record(model: sys_tenant_datasource::Model) -> TenantMetadataRecord {
        let isolation_level = match model.isolation_level {
            SystemTenantIsolationLevel::SharedRow => TenantIsolationLevel::SharedRow,
            SystemTenantIsolationLevel::SeparateTable => TenantIsolationLevel::SeparateTable,
            SystemTenantIsolationLevel::SeparateSchema => TenantIsolationLevel::SeparateSchema,
            SystemTenantIsolationLevel::SeparateDatabase => TenantIsolationLevel::SeparateDatabase,
        };

        let status = match model.status {
            SystemTenantDatasourceStatus::Active => "active",
            SystemTenantDatasourceStatus::Inactive => "inactive",
            SystemTenantDatasourceStatus::Provisioning => "provisioning",
            SystemTenantDatasourceStatus::Error => "error",
        };

        TenantMetadataRecord {
            tenant_id: model.tenant_id,
            isolation_level,
            status: Some(status.to_string()),
            schema_name: model.schema_name,
            datasource_name: model.datasource_name,
            db_uri: model.db_uri,
            db_enable_logging: model.db_enable_logging,
            db_min_conns: model
                .db_min_conns
                .and_then(|value| u32::try_from(value).ok()),
            db_max_conns: model
                .db_max_conns
                .and_then(|value| u32::try_from(value).ok()),
            db_connect_timeout_ms: model
                .db_connect_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_idle_timeout_ms: model
                .db_idle_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_acquire_timeout_ms: model
                .db_acquire_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_test_before_acquire: model.db_test_before_acquire,
        }
    }
}

#[component]
pub fn sys_tenant_datasource_metadata_loader() -> Arc<dyn TenantMetadataLoader> {
    Arc::new(SeaOrmTenantMetadataLoader::<SysTenantDatasourceSchema>::new())
}
