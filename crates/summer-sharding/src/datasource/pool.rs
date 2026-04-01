use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

use async_trait::async_trait;
use parking_lot::RwLock;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

use crate::{
    config::{DataSourceConfig, DataSourceRole, ShardingConfig},
    datasource::{
        DataSourceDiscovery, DataSourceHealth, DataSourceRouteState, SlowQueryMetric,
        record_slow_query, set_route_state,
    },
    error::{Result, ShardingError},
    execute::RawStatementExecutor,
    tenant::TenantMetadataStore,
};

#[derive(Clone)]
pub struct DataSourcePool {
    config: ShardingConfig,
    connections: Arc<RwLock<BTreeMap<String, DatabaseConnection>>>,
}

impl std::fmt::Debug for DataSourcePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataSourcePool")
            .field(
                "datasources",
                &self.connections.read().keys().cloned().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl DataSourcePool {
    pub async fn build(config: Arc<ShardingConfig>) -> Result<Self> {
        let mut connections = BTreeMap::new();
        for (name, datasource) in &config.datasources {
            let connection = connect_datasource(datasource).await?;
            connections.insert(name.clone(), connection);
        }
        Ok(Self {
            config: config.as_ref().clone(),
            connections: Arc::new(RwLock::new(connections)),
        })
    }

    pub fn from_connections(
        config: Arc<ShardingConfig>,
        connections: BTreeMap<String, DatabaseConnection>,
    ) -> Result<Self> {
        for name in config.datasources.keys() {
            if !connections.contains_key(name) {
                return Err(ShardingError::DataSourceNotFound(name.clone()));
            }
        }
        Ok(Self {
            config: config.as_ref().clone(),
            connections: Arc::new(RwLock::new(connections)),
        })
    }

    pub fn connection(&self, datasource: &str) -> Result<DatabaseConnection> {
        self.connections
            .read()
            .get(datasource)
            .cloned()
            .ok_or_else(|| ShardingError::DataSourceNotFound(datasource.to_string()))
    }

    pub async fn upsert_connection(&self, name: &str, datasource: &DataSourceConfig) -> Result<()> {
        let connection = connect_datasource(datasource).await?;
        self.connections
            .write()
            .insert(name.to_string(), connection);
        Ok(())
    }

    pub fn remove_connection(&self, name: &str) -> Option<DatabaseConnection> {
        self.connections.write().remove(name)
    }

    pub async fn sync_tenant_datasources(&self, metadata: &TenantMetadataStore) -> Result<()> {
        let dynamic_candidates = metadata.dynamic_datasources();
        for (name, datasource) in &dynamic_candidates {
            self.upsert_connection(name.as_str(), datasource).await?;
        }

        let dynamic_names = dynamic_candidates
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();

        let static_names = self
            .config
            .datasources
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut guard = self.connections.write();
        let to_remove = guard
            .keys()
            .filter(|name| !static_names.contains(*name) && !dynamic_names.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in to_remove {
            guard.remove(&name);
        }

        Ok(())
    }

    pub async fn discovery(&self, primary_hint: Option<&str>) -> DataSourceDiscovery {
        let health = self.health_check().await;
        DataSourceDiscovery::detect(&health, primary_hint)
    }

    pub async fn refresh_read_write_route_states(&self) -> Vec<DataSourceRouteState> {
        let health = self.health_check().await;
        self.config
            .read_write_splitting
            .rules
            .iter()
            .map(|rule| {
                let candidates = std::iter::once(rule.primary.clone())
                    .chain(rule.replicas.iter().cloned())
                    .collect::<BTreeSet<_>>();
                let state = DataSourceDiscovery::detect_scoped(
                    &health,
                    Some(rule.primary.as_str()),
                    Some(&candidates),
                )
                .into_route_state(
                    rule.name.as_str(),
                    rule.primary.as_str(),
                    &rule.replicas,
                );
                set_route_state(rule.primary.as_str(), state.clone());
                state
            })
            .collect()
    }

    pub fn primary_datasource_names(&self) -> Vec<String> {
        let mut names = self
            .config
            .datasources
            .iter()
            .filter(|(_, config)| config.role == DataSourceRole::Primary)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() {
            names.extend(self.connections.read().keys().cloned());
        }
        names
    }

    pub fn datasource_names(&self) -> Vec<String> {
        self.connections.read().keys().cloned().collect()
    }

    pub async fn health_check(&self) -> Vec<DataSourceHealth> {
        let snapshot = self.connections.read().clone();
        let mut health = Vec::with_capacity(snapshot.len());
        let slow_threshold_ms = self.config.audit.slow_query_threshold_ms as u128;
        for (datasource, connection) in snapshot {
            let datasource_name = datasource.clone();
            let started = Instant::now();
            let status = connection
                .ping()
                .await
                .map(|_| {
                    let elapsed_ms = started.elapsed().as_millis();
                    if elapsed_ms >= slow_threshold_ms {
                        record_slow_query(SlowQueryMetric {
                            datasource: datasource_name.clone(),
                            elapsed_ms,
                            threshold_ms: slow_threshold_ms,
                            reason: "health_check".to_string(),
                        });
                    }
                    DataSourceHealth {
                        datasource,
                        reachable: true,
                        error: None,
                        latency_ms: Some(elapsed_ms),
                    }
                })
                .unwrap_or_else(|err| {
                    let elapsed_ms = started.elapsed().as_millis();
                    DataSourceHealth {
                        datasource: datasource_name,
                        reachable: false,
                        error: Some(err.to_string()),
                        latency_ms: Some(elapsed_ms),
                    }
                });
            health.push(status);
        }
        health
    }
}

async fn connect_datasource(datasource: &DataSourceConfig) -> Result<DatabaseConnection> {
    Ok(Database::connect(datasource.connect_options()).await?)
}

#[async_trait]
impl RawStatementExecutor for DataSourcePool {
    async fn execute_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<sea_orm::ExecResult, sea_orm::DbErr> {
        self.connection(datasource)?.execute_raw(stmt).await
    }

    async fn query_one_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
        self.connection(datasource)?.query_one_raw(stmt).await
    }

    async fn query_all_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Vec<sea_orm::QueryResult>, sea_orm::DbErr> {
        self.connection(datasource)?.query_all_raw(stmt).await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use sea_orm::{DbBackend, MockDatabase};

    use crate::{
        config::{DataSourceConfig, DataSourceRole, ShardingConfig, TenantIsolationLevel},
        datasource::{DataSourcePool, clear_route_states, route_state},
        tenant::{TenantMetadataRecord, TenantMetadataStore},
    };

    #[tokio::test]
    async fn sync_tenant_datasources_removes_inactive_dynamic_connections() {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "primary".to_string(),
            DataSourceConfig {
                role: DataSourceRole::Primary,
                ..DataSourceConfig::new("sqlite::memory:")
            },
        );

        let config = Arc::new(ShardingConfig {
            datasources,
            ..Default::default()
        });

        let primary_conn = MockDatabase::new(DbBackend::Postgres).into_connection();
        let dynamic_conn = MockDatabase::new(DbBackend::Postgres).into_connection();

        let mut connections = BTreeMap::new();
        connections.insert("primary".to_string(), primary_conn);
        connections.insert("tenant_t001".to_string(), dynamic_conn);

        let pool = DataSourcePool::from_connections(config.clone(), connections).expect("pool");

        let record = TenantMetadataRecord {
            tenant_id: "T-001".to_string(),
            isolation_level: TenantIsolationLevel::SeparateDatabase,
            status: Some("inactive".to_string()),
            schema_name: None,
            datasource_name: Some("tenant_t001".to_string()),
            db_uri: Some("sqlite::memory:".to_string()),
            db_enable_logging: None,
            db_min_conns: None,
            db_max_conns: None,
            db_connect_timeout_ms: None,
            db_idle_timeout_ms: None,
            db_acquire_timeout_ms: None,
            db_test_before_acquire: None,
        };

        let store = TenantMetadataStore::from_records(vec![record]);

        pool.sync_tenant_datasources(&store).await.expect("sync");

        assert!(pool.connection("tenant_t001").is_err());
        assert!(pool.connection("primary").is_ok());
    }

    #[tokio::test]
    async fn discovery_detects_primary_and_replicas() {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "primary".to_string(),
            DataSourceConfig {
                role: DataSourceRole::Primary,
                ..DataSourceConfig::new("mock://primary")
            },
        );
        datasources.insert(
            "replica".to_string(),
            DataSourceConfig {
                role: DataSourceRole::Replica,
                ..DataSourceConfig::new("mock://replica")
            },
        );

        let config = Arc::new(ShardingConfig {
            datasources,
            ..Default::default()
        });

        let pool = DataSourcePool::from_connections(
            config,
            BTreeMap::from([
                (
                    "primary".to_string(),
                    MockDatabase::new(DbBackend::Postgres).into_connection(),
                ),
                (
                    "replica".to_string(),
                    MockDatabase::new(DbBackend::Postgres).into_connection(),
                ),
            ]),
        )
        .expect("build pool");
        let discovery = pool.discovery(Some("primary")).await;
        assert_eq!(discovery.primary.as_deref(), Some("primary"));
        assert_eq!(discovery.replicas, vec!["replica".to_string()]);
        assert!(discovery.unhealthy.is_empty());
    }

    #[tokio::test]
    async fn refresh_read_write_route_states_writes_runtime_state() {
        clear_route_states();
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
            [datasources.primary]
            uri = "mock://primary"
            role = "primary"

            [datasources.replica]
            uri = "mock://replica"
            role = "replica"

            [read_write_splitting]
            enabled = true

              [[read_write_splitting.rules]]
              name = "ai-rw"
              primary = "primary"
              replicas = ["replica"]
              load_balance = "round_robin"
            "#,
            )
            .expect("config"),
        );
        let pool = DataSourcePool::from_connections(
            config,
            BTreeMap::from([
                (
                    "primary".to_string(),
                    MockDatabase::new(DbBackend::Postgres).into_connection(),
                ),
                (
                    "replica".to_string(),
                    MockDatabase::new(DbBackend::Postgres).into_connection(),
                ),
            ]),
        )
        .expect("pool");

        let states = pool.refresh_read_write_route_states().await;
        assert_eq!(states.len(), 1);
        assert_eq!(
            route_state("primary").expect("state").healthy_replicas,
            vec!["replica".to_string()]
        );
        clear_route_states();
    }
}
