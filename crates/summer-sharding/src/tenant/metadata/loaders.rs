use std::marker::PhantomData;

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use super::{
    TenantMetadataLoader, TenantMetadataRecord, TenantMetadataSchema, TenantMetadataStore,
};
use crate::{config::TenantIsolationLevel, error::Result};

/// Built-in tenant metadata loader.
///
/// - Tries to load from Postgres table `tenant.tenant_datasource` (the default metadata table in this repo).
/// - If the table does not exist, returns an empty store instead of failing startup.
#[derive(Debug, Clone, Copy, Default)]
pub struct SysTenantDatasourceMetadataLoader;

#[async_trait]
impl TenantMetadataLoader for SysTenantDatasourceMetadataLoader {
    async fn load_store(&self, connection: &DatabaseConnection) -> Result<TenantMetadataStore> {
        // Keep the SQL minimal: we only read the columns we actually map into TenantMetadataRecord.
        let backend = connection.get_database_backend();
        let table = match backend {
            DbBackend::Postgres => "tenant.tenant_datasource",
            // Best-effort fallback for other backends. If the table doesn't exist, we'll return empty.
            _ => "tenant_datasource",
        };

        let stmt = Statement::from_string(
            backend,
            format!(
                r#"
SELECT
  tenant_id,
  isolation_level,
  status,
  schema_name,
  datasource_name,
  db_uri,
  db_enable_logging,
  db_min_conns,
  db_max_conns,
  db_connect_timeout_ms,
  db_idle_timeout_ms,
  db_acquire_timeout_ms,
  db_test_before_acquire
FROM {table}
"#
            ),
        );

        let rows = match connection.query_all_raw(stmt).await {
            Ok(rows) => rows,
            Err(err) => {
                let message = err.to_string();
                // Postgres: `relation "tenant.tenant_datasource" does not exist`
                // SQLite: `no such table: tenant_datasource`
                // MySQL: `Table '...' doesn't exist`
                if message.contains("does not exist") || message.contains("no such table") {
                    tracing::warn!(
                        error = %message,
                        "tenant metadata table not found, falling back to empty store"
                    );
                    return Ok(TenantMetadataStore::new());
                }
                return Err(err.into());
            }
        };

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let tenant_id = row.try_get::<String>("", "tenant_id")?;
            let isolation_code = row.try_get::<i16>("", "isolation_level")?;
            let isolation_level = TenantIsolationLevel::from_code(isolation_code)
                .unwrap_or(TenantIsolationLevel::SharedRow);

            let record = TenantMetadataRecord {
                tenant_id,
                isolation_level,
                status: row.try_get::<Option<String>>("", "status")?,
                schema_name: row.try_get::<Option<String>>("", "schema_name")?,
                datasource_name: row.try_get::<Option<String>>("", "datasource_name")?,
                db_uri: row.try_get::<Option<String>>("", "db_uri")?,
                db_enable_logging: row.try_get::<Option<bool>>("", "db_enable_logging")?,
                db_min_conns: row
                    .try_get::<Option<i32>>("", "db_min_conns")?
                    .and_then(|value| u32::try_from(value).ok()),
                db_max_conns: row
                    .try_get::<Option<i32>>("", "db_max_conns")?
                    .and_then(|value| u32::try_from(value).ok()),
                db_connect_timeout_ms: row
                    .try_get::<Option<i64>>("", "db_connect_timeout_ms")?
                    .and_then(|value| u64::try_from(value).ok()),
                db_idle_timeout_ms: row
                    .try_get::<Option<i64>>("", "db_idle_timeout_ms")?
                    .and_then(|value| u64::try_from(value).ok()),
                db_acquire_timeout_ms: row
                    .try_get::<Option<i64>>("", "db_acquire_timeout_ms")?
                    .and_then(|value| u64::try_from(value).ok()),
                db_test_before_acquire: row
                    .try_get::<Option<bool>>("", "db_test_before_acquire")?,
            };
            records.push(record);
        }

        Ok(TenantMetadataStore::from_records(records))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SeaOrmTenantMetadataLoader<S> {
    _schema: PhantomData<S>,
}

impl<S> SeaOrmTenantMetadataLoader<S> {
    pub fn new() -> Self {
        Self {
            _schema: PhantomData,
        }
    }
}

#[async_trait]
impl<S> TenantMetadataLoader for SeaOrmTenantMetadataLoader<S>
where
    S: TenantMetadataSchema + Send + Sync + 'static,
{
    async fn load_store(&self, connection: &DatabaseConnection) -> Result<TenantMetadataStore> {
        TenantMetadataStore::load_from_connection::<S>(connection).await
    }
}
