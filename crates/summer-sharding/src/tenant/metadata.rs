use std::collections::BTreeMap;
use std::marker::PhantomData;

use async_trait::async_trait;
use parking_lot::RwLock;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait, Statement};
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataSourceConfig, DataSourceRole, TenantIsolationLevel},
    error::Result,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMetadataRecord {
    pub tenant_id: String,
    pub isolation_level: TenantIsolationLevel,
    pub status: Option<String>,
    pub schema_name: Option<String>,
    pub datasource_name: Option<String>,
    pub db_uri: Option<String>,
    pub db_enable_logging: Option<bool>,
    pub db_min_conns: Option<u32>,
    pub db_max_conns: Option<u32>,
    pub db_connect_timeout_ms: Option<u64>,
    pub db_idle_timeout_ms: Option<u64>,
    pub db_acquire_timeout_ms: Option<u64>,
    pub db_test_before_acquire: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantMetadataEventKind {
    Upsert,
    Delete,
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMetadataEvent {
    pub event: TenantMetadataEventKind,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub record: Option<TenantMetadataRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantMetadataApplyOutcome {
    Applied,
    ReloadRequired,
}

#[derive(Debug, Default)]
pub struct TenantMetadataStore {
    records: RwLock<BTreeMap<String, TenantMetadataRecord>>,
}

pub trait TenantMetadataSchema {
    type Entity: EntityTrait;

    fn into_record(model: <Self::Entity as EntityTrait>::Model) -> TenantMetadataRecord;

    fn load_models(
        connection: &DatabaseConnection,
    ) -> impl std::future::Future<
        Output = std::result::Result<Vec<<Self::Entity as EntityTrait>::Model>, DbErr>,
    > + Send {
        async move { Self::Entity::find().all(connection).await }
    }
}

#[async_trait]
pub trait TenantMetadataLoader: Send + Sync + 'static {
    async fn load_store(&self, connection: &DatabaseConnection) -> Result<TenantMetadataStore>;
}

/// Built-in tenant metadata loader.
///
/// - Tries to load from Postgres table `sys.tenant_datasource` (the default metadata table in this repo).
/// - If the table does not exist, returns an empty store instead of failing startup.
#[derive(Debug, Clone, Copy, Default)]
pub struct SysTenantDatasourceMetadataLoader;

#[async_trait]
impl TenantMetadataLoader for SysTenantDatasourceMetadataLoader {
    async fn load_store(&self, connection: &DatabaseConnection) -> Result<TenantMetadataStore> {
        // Keep the SQL minimal: we only read the columns we actually map into TenantMetadataRecord.
        let backend = connection.get_database_backend();
        let table = match backend {
            DbBackend::Postgres => "sys.tenant_datasource",
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
                // Postgres: `relation "sys.tenant_datasource" does not exist`
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

impl TenantMetadataStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_records(records: impl IntoIterator<Item = TenantMetadataRecord>) -> Self {
        let store = Self::default();
        for record in records {
            store.upsert(record);
        }
        store
    }

    pub fn upsert(&self, record: TenantMetadataRecord) {
        self.records
            .write()
            .insert(record.tenant_id.clone(), record);
    }

    pub fn remove(&self, tenant_id: &str) -> Option<TenantMetadataRecord> {
        self.records.write().remove(tenant_id)
    }

    pub fn get(&self, tenant_id: &str) -> Option<TenantMetadataRecord> {
        self.records.read().get(tenant_id).cloned()
    }

    pub fn list(&self) -> Vec<TenantMetadataRecord> {
        self.records.read().values().cloned().collect()
    }

    pub async fn load_from_connection<S>(connection: &DatabaseConnection) -> Result<Self>
    where
        S: TenantMetadataSchema,
    {
        let models = S::load_models(connection).await?;
        Ok(Self::from_records(models.into_iter().map(S::into_record)))
    }

    pub async fn refresh_from_connection<S>(&self, connection: &DatabaseConnection) -> Result<()>
    where
        S: TenantMetadataSchema,
    {
        let other = Self::load_from_connection::<S>(connection).await?;
        *self.records.write() = other.records.read().clone();
        Ok(())
    }

    pub async fn replace_with_loader(
        &self,
        connection: &DatabaseConnection,
        loader: &dyn TenantMetadataLoader,
    ) -> Result<()> {
        let other = loader.load_store(connection).await?;
        *self.records.write() = other.records.read().clone();
        Ok(())
    }

    pub fn apply_event(&self, event: TenantMetadataEvent) -> TenantMetadataApplyOutcome {
        match event.event {
            TenantMetadataEventKind::Upsert => {
                if let Some(record) = event.record {
                    self.upsert(record);
                    TenantMetadataApplyOutcome::Applied
                } else {
                    TenantMetadataApplyOutcome::ReloadRequired
                }
            }
            TenantMetadataEventKind::Delete => {
                if let Some(tenant_id) = event
                    .tenant_id
                    .or_else(|| event.record.map(|r| r.tenant_id))
                {
                    self.remove(tenant_id.as_str());
                    TenantMetadataApplyOutcome::Applied
                } else {
                    TenantMetadataApplyOutcome::ReloadRequired
                }
            }
            TenantMetadataEventKind::Reload => TenantMetadataApplyOutcome::ReloadRequired,
        }
    }

    pub fn apply_notification_payload(&self, payload: &str) -> Result<TenantMetadataApplyOutcome> {
        let event: TenantMetadataEvent = serde_json::from_str(payload)
            .map_err(|err| crate::error::ShardingError::Parse(err.to_string()))?;
        Ok(self.apply_event(event))
    }

    pub fn dynamic_datasources(&self) -> Vec<(String, DataSourceConfig)> {
        self.records
            .read()
            .values()
            .filter(|record| match record.status.as_deref() {
                Some(status) => status.eq_ignore_ascii_case("active"),
                None => true,
            })
            .filter_map(|record| {
                record.db_uri.as_ref().map(|uri| {
                    (
                        record
                            .datasource_name
                            .clone()
                            .unwrap_or_else(|| format!("tenant_{}", record.tenant_id)),
                        DataSourceConfig {
                            schema: record.schema_name.clone(),
                            role: DataSourceRole::Primary,
                            enable_logging: record.db_enable_logging.unwrap_or(false),
                            min_connections: record.db_min_conns.unwrap_or(1),
                            max_connections: record.db_max_conns.unwrap_or(10),
                            connect_timeout: record.db_connect_timeout_ms,
                            idle_timeout: record.db_idle_timeout_ms,
                            acquire_timeout: record.db_acquire_timeout_ms,
                            test_before_acquire: record.db_test_before_acquire.unwrap_or(true),
                            ..DataSourceConfig::new(uri.clone())
                        },
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, MockDatabase};

    use crate::config::{DataSourceConfig, DataSourceRole};
    use crate::tenant::{
        TenantMetadataApplyOutcome, TenantMetadataEvent, TenantMetadataEventKind,
        TenantMetadataRecord, TenantMetadataSchema, TenantMetadataStore,
    };

    use crate::config::TenantIsolationLevel;

    mod test_tenant_datasource_entity {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(schema_name = "sys", table_name = "tenant_datasource")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub tenant_id: String,
            pub isolation_level: i16,
            pub status: Option<String>,
            pub schema_name: Option<String>,
            pub datasource_name: Option<String>,
            pub db_uri: Option<String>,
            pub db_enable_logging: Option<bool>,
            pub db_min_conns: Option<i32>,
            pub db_max_conns: Option<i32>,
            pub db_connect_timeout_ms: Option<i64>,
            pub db_idle_timeout_ms: Option<i64>,
            pub db_acquire_timeout_ms: Option<i64>,
            pub db_test_before_acquire: Option<bool>,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    struct TestTenantMetadataSchema;

    impl TenantMetadataSchema for TestTenantMetadataSchema {
        type Entity = test_tenant_datasource_entity::Entity;
        fn into_record(model: test_tenant_datasource_entity::Model) -> TenantMetadataRecord {
            let isolation_level = match model.isolation_level {
                1 => TenantIsolationLevel::SharedRow,
                2 => TenantIsolationLevel::SeparateTable,
                3 => TenantIsolationLevel::SeparateSchema,
                4 => TenantIsolationLevel::SeparateDatabase,
                _ => TenantIsolationLevel::SharedRow,
            };

            TenantMetadataRecord {
                tenant_id: model.tenant_id,
                isolation_level,
                status: model.status,
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

    #[tokio::test]
    async fn metadata_store_loads_models_from_database() {
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[test_tenant_datasource_entity::Model {
                id: 1,
                tenant_id: "T-001".to_string(),
                isolation_level: 3,
                status: Some("active".to_string()),
                schema_name: Some("tenant_001".to_string()),
                datasource_name: None,
                db_uri: None,
                db_enable_logging: Some(true),
                db_min_conns: Some(3),
                db_max_conns: None,
                db_connect_timeout_ms: Some(1_500),
                db_idle_timeout_ms: Some(2_500),
                db_acquire_timeout_ms: Some(3_500),
                db_test_before_acquire: Some(false),
            }]])
            .into_connection();

        let store =
            TenantMetadataStore::load_from_connection::<TestTenantMetadataSchema>(&connection)
                .await
                .expect("metadata");
        let record = store.get("T-001").expect("tenant");
        assert_eq!(record.schema_name.as_deref(), Some("tenant_001"));
        assert_eq!(record.db_enable_logging, Some(true));
        assert_eq!(record.db_min_conns, Some(3));
        assert_eq!(record.db_connect_timeout_ms, Some(1_500));
        assert_eq!(record.db_idle_timeout_ms, Some(2_500));
        assert_eq!(record.db_acquire_timeout_ms, Some(3_500));
        assert_eq!(record.db_test_before_acquire, Some(false));
    }

    #[tokio::test]
    async fn metadata_store_loads_models_from_generic_schema() {
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[test_tenant_datasource_entity::Model {
                id: 10,
                tenant_id: "T-010".to_string(),
                isolation_level: 4,
                status: Some("active".to_string()),
                schema_name: None,
                datasource_name: Some("tenant_t010".to_string()),
                db_uri: Some("postgres://tenant-t010".to_string()),
                db_enable_logging: Some(true),
                db_min_conns: Some(2),
                db_max_conns: Some(8),
                db_connect_timeout_ms: Some(1_000),
                db_idle_timeout_ms: Some(2_000),
                db_acquire_timeout_ms: Some(3_000),
                db_test_before_acquire: Some(false),
            }]])
            .into_connection();

        let store =
            TenantMetadataStore::load_from_connection::<TestTenantMetadataSchema>(&connection)
                .await
                .expect("metadata");
        let record = store.get("T-010").expect("tenant");

        assert_eq!(
            record.isolation_level,
            TenantIsolationLevel::SeparateDatabase
        );
        assert_eq!(record.datasource_name.as_deref(), Some("tenant_t010"));
        assert_eq!(record.db_uri.as_deref(), Some("postgres://tenant-t010"));
        assert_eq!(record.db_max_conns, Some(8));
        assert_eq!(record.db_test_before_acquire, Some(false));
    }

    #[test]
    fn metadata_store_applies_notification_payload() {
        let store = TenantMetadataStore::new();
        let outcome = store
            .apply_notification_payload(
                serde_json::to_string(&TenantMetadataEvent {
                    event: TenantMetadataEventKind::Upsert,
                    tenant_id: Some("T-002".to_string()),
                    record: Some(TenantMetadataRecord {
                        tenant_id: "T-002".to_string(),
                        isolation_level: crate::config::TenantIsolationLevel::SeparateDatabase,
                        status: Some("active".to_string()),
                        schema_name: None,
                        datasource_name: Some("tenant_t002".to_string()),
                        db_uri: Some("postgres://tenant".to_string()),
                        db_enable_logging: Some(true),
                        db_min_conns: Some(2),
                        db_max_conns: Some(10),
                        db_connect_timeout_ms: Some(1_000),
                        db_idle_timeout_ms: Some(2_000),
                        db_acquire_timeout_ms: Some(3_000),
                        db_test_before_acquire: Some(false),
                    }),
                })
                .expect("json")
                .as_str(),
            )
            .expect("payload");

        assert_eq!(outcome, TenantMetadataApplyOutcome::Applied);
        assert!(store.get("T-002").is_some());
    }

    #[test]
    fn dynamic_datasources_ignores_inactive_statuses() {
        let store = TenantMetadataStore::from_records(vec![TenantMetadataRecord {
            tenant_id: "T-003".to_string(),
            isolation_level: TenantIsolationLevel::SeparateDatabase,
            status: Some("inactive".to_string()),
            schema_name: None,
            datasource_name: Some("tenant_t003".to_string()),
            db_uri: Some("postgres://tenant-db".to_string()),
            db_enable_logging: None,
            db_min_conns: None,
            db_max_conns: Some(10),
            db_connect_timeout_ms: None,
            db_idle_timeout_ms: None,
            db_acquire_timeout_ms: None,
            db_test_before_acquire: None,
        }]);
        assert!(store.dynamic_datasources().is_empty());
    }

    #[test]
    fn dynamic_datasources_includes_active_statuses() {
        let store = TenantMetadataStore::from_records(vec![TenantMetadataRecord {
            tenant_id: "T-004".to_string(),
            isolation_level: TenantIsolationLevel::SeparateDatabase,
            status: Some("active".to_string()),
            schema_name: None,
            datasource_name: Some("tenant_t004".to_string()),
            db_uri: Some("postgres://tenant-db".to_string()),
            db_enable_logging: None,
            db_min_conns: None,
            db_max_conns: Some(10),
            db_connect_timeout_ms: None,
            db_idle_timeout_ms: None,
            db_acquire_timeout_ms: None,
            db_test_before_acquire: None,
        }]);
        let datasources = store.dynamic_datasources();
        assert_eq!(datasources.len(), 1);
        assert_eq!(datasources[0].0, "tenant_t004");
        assert_eq!(datasources[0].1.max_connections, 10);
    }

    #[test]
    fn dynamic_datasources_defaults_missing_status_to_active() {
        let store = TenantMetadataStore::from_records(vec![TenantMetadataRecord {
            tenant_id: "T-005".to_string(),
            isolation_level: TenantIsolationLevel::SeparateDatabase,
            status: None,
            schema_name: None,
            datasource_name: Some("tenant_t005".to_string()),
            db_uri: Some("postgres://tenant-db".to_string()),
            db_enable_logging: None,
            db_min_conns: None,
            db_max_conns: Some(10),
            db_connect_timeout_ms: None,
            db_idle_timeout_ms: None,
            db_acquire_timeout_ms: None,
            db_test_before_acquire: None,
        }]);
        let datasources = store.dynamic_datasources();
        assert_eq!(datasources.len(), 1);
        assert_eq!(datasources[0].0, "tenant_t005");
    }

    #[test]
    fn dynamic_datasources_map_db_pool_settings_into_datasource_config() {
        let store = TenantMetadataStore::from_records(vec![TenantMetadataRecord {
            tenant_id: "T-006".to_string(),
            isolation_level: TenantIsolationLevel::SeparateDatabase,
            status: Some("active".to_string()),
            schema_name: Some("tenant_006".to_string()),
            datasource_name: Some("tenant_t006".to_string()),
            db_uri: Some("postgres://tenant-006".to_string()),
            db_enable_logging: Some(true),
            db_min_conns: Some(3),
            db_max_conns: Some(24),
            db_connect_timeout_ms: Some(1_500),
            db_idle_timeout_ms: Some(2_500),
            db_acquire_timeout_ms: Some(3_500),
            db_test_before_acquire: Some(false),
        }]);
        let datasources = store.dynamic_datasources();
        assert_eq!(datasources.len(), 1);
        assert_eq!(
            datasources[0].1,
            DataSourceConfig {
                schema: Some("tenant_006".to_string()),
                role: DataSourceRole::Primary,
                enable_logging: true,
                min_connections: 3,
                max_connections: 24,
                connect_timeout: Some(1_500),
                idle_timeout: Some(2_500),
                acquire_timeout: Some(3_500),
                test_before_acquire: false,
                ..DataSourceConfig::new("postgres://tenant-006")
            }
        );
    }
}
