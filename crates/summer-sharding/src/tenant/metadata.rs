use std::{collections::BTreeMap, sync::Arc};

use parking_lot::RwLock;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
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

impl TenantMetadataStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn from_records(records: impl IntoIterator<Item = TenantMetadataRecord>) -> Arc<Self> {
        let store = Self::default();
        for record in records {
            store.upsert(record);
        }
        Arc::new(store)
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

    pub async fn load_from_connection(connection: &DatabaseConnection) -> Result<Arc<Self>> {
        let rows = connection
            .query_all_raw(Statement::from_string(
                connection.get_database_backend(),
                "SELECT tenant_id, isolation_level, status, schema_name, datasource_name, db_uri, db_enable_logging, db_min_conns, db_max_conns, db_connect_timeout_ms, db_idle_timeout_ms, db_acquire_timeout_ms, db_test_before_acquire FROM sys.tenant_datasource",
            ))
            .await?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let isolation_level = row
                .try_get::<Option<i16>>("", "isolation_level")
                .ok()
                .flatten()
                .and_then(parse_isolation)
                .unwrap_or(TenantIsolationLevel::SharedRow);
            records.push(TenantMetadataRecord {
                tenant_id: row.try_get::<String>("", "tenant_id")?,
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
            });
        }
        Ok(Self::from_records(records))
    }

    pub async fn refresh_from_connection(&self, connection: &DatabaseConnection) -> Result<()> {
        let other = Self::load_from_connection(connection).await?;
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

fn parse_isolation(value: i16) -> Option<TenantIsolationLevel> {
    match value {
        1 => Some(TenantIsolationLevel::SharedRow),
        2 => Some(TenantIsolationLevel::SeparateTable),
        3 => Some(TenantIsolationLevel::SeparateSchema),
        4 => Some(TenantIsolationLevel::SeparateDatabase),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, MockDatabase};

    use crate::config::{DataSourceConfig, DataSourceRole};
    use crate::tenant::{
        TenantMetadataApplyOutcome, TenantMetadataEvent, TenantMetadataEventKind,
        TenantMetadataRecord, TenantMetadataStore,
    };

    use crate::config::TenantIsolationLevel;

    #[tokio::test]
    async fn metadata_store_loads_rows_from_database() {
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[std::collections::BTreeMap::from([
                ("tenant_id".to_string(), "T-001".into()),
                ("isolation_level".to_string(), Some(3_i16).into()),
                ("status".to_string(), Some("active".to_string()).into()),
                (
                    "schema_name".to_string(),
                    Some("tenant_001".to_string()).into(),
                ),
                ("datasource_name".to_string(), Option::<String>::None.into()),
                ("db_uri".to_string(), Option::<String>::None.into()),
                ("db_enable_logging".to_string(), Some(true).into()),
                ("db_min_conns".to_string(), Some(3_i32).into()),
                ("db_max_conns".to_string(), Option::<i32>::None.into()),
                ("db_connect_timeout_ms".to_string(), Some(1_500_i64).into()),
                ("db_idle_timeout_ms".to_string(), Some(2_500_i64).into()),
                ("db_acquire_timeout_ms".to_string(), Some(3_500_i64).into()),
                ("db_test_before_acquire".to_string(), Some(false).into()),
            ])]])
            .into_connection();

        let store = TenantMetadataStore::load_from_connection(&connection)
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
