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
    pub db_max_conns: Option<u32>,
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
                "SELECT tenant_id, isolation_level, status, schema_name, datasource_name, db_uri, db_max_conns FROM sys.tenant_datasource",
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
                db_max_conns: row
                    .try_get::<Option<i32>>("", "db_max_conns")?
                    .map(|value| value as u32),
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
                            uri: uri.clone(),
                            schema: record.schema_name.clone(),
                            role: DataSourceRole::Primary,
                            weight: 1,
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
                ("db_max_conns".to_string(), Option::<i32>::None.into()),
            ])]])
            .into_connection();

        let store = TenantMetadataStore::load_from_connection(&connection)
            .await
            .expect("metadata");
        let record = store.get("T-001").expect("tenant");
        assert_eq!(record.schema_name.as_deref(), Some("tenant_001"));
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
                        db_max_conns: Some(10),
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
            db_max_conns: Some(10),
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
            db_max_conns: Some(10),
        }]);
        let datasources = store.dynamic_datasources();
        assert_eq!(datasources.len(), 1);
        assert_eq!(datasources[0].0, "tenant_t004");
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
            db_max_conns: Some(10),
        }]);
        let datasources = store.dynamic_datasources();
        assert_eq!(datasources.len(), 1);
        assert_eq!(datasources[0].0, "tenant_t005");
    }
}
