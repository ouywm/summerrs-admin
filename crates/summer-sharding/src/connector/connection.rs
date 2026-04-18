use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Statement,
};

use crate::{
    audit::{DefaultSqlAuditor, SqlAuditor},
    config::ShardingConfig,
    connector::{ShardingAccessContext, ShardingHint},
    datasource::{DataSourceHealth, DataSourcePool, DataSourceRouteState, route_state},
    error::Result,
    execute::{Executor, ScatterGatherExecutor},
    keygen::{KeyGenerator, KeyGeneratorRegistry},
    lookup::{LookupDefinition, LookupIndex},
    merge::{DefaultResultMerger, ResultMerger},
    rewrite::{DefaultSqlRewriter, SqlRewriter},
    rewrite_plugin::PluginRegistry,
    router::{DefaultSqlRouter, SqlRouter},
    shadow::ShadowRouter,
    tenant::{TenantMetadataLoader, TenantMetadataStore, TenantRouter},
};

mod audit;
mod exec;
mod lookup;
mod metadata;
mod overrides;

#[derive(Clone)]
pub struct ShardingConnection {
    pub(crate) inner: Arc<ShardingConnectionInner>,
    pub(crate) hint_override: Option<ShardingHint>,
    pub(crate) access_context_override: Option<ShardingAccessContext>,
    pub(crate) tenant_override: Option<crate::tenant::TenantContext>,
    pub(crate) shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
}

#[derive(Clone, Default)]
pub(crate) struct ExecutionOverrides {
    pub(crate) hint: Option<ShardingHint>,
    pub(crate) access_context: Option<ShardingAccessContext>,
    pub(crate) tenant: Option<crate::tenant::TenantContext>,
    pub(crate) shadow_headers: Option<Arc<BTreeMap<String, String>>>,
}

pub(crate) struct ShardingConnectionInner {
    pub(crate) config: Arc<ShardingConfig>,
    pub(crate) pool: DataSourcePool,
    pub(crate) router: Box<dyn SqlRouter>,
    pub(crate) rewriter: Box<dyn SqlRewriter>,
    pub(crate) executor: Box<dyn Executor>,
    pub(crate) merger: Box<dyn ResultMerger>,
    pub(crate) key_generators: BTreeMap<String, Arc<dyn KeyGenerator>>,
    pub(crate) lookup_index: Arc<LookupIndex>,
    pub(crate) tenant_metadata: Arc<TenantMetadataStore>,
    pub(crate) metadata_loader: OnceLock<Arc<dyn TenantMetadataLoader>>,
    pub(crate) tenant_router: TenantRouter,
    pub(crate) shadow_router: ShadowRouter,
    pub(crate) auditor: Arc<dyn SqlAuditor>,
    /// SQL 改写插件注册表（可选，由应用层通过 ShardingRewriteConfigurator 注入）
    pub(crate) plugin_registry: OnceLock<PluginRegistry>,
}

impl std::fmt::Debug for ShardingConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardingConnection").finish()
    }
}

impl std::fmt::Debug for ShardingConnectionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardingConnectionInner")
            .field("datasources", &self.pool)
            .finish()
    }
}

impl ShardingConnection {
    pub async fn build(
        config: ShardingConfig,
        bootstrap_connection: DatabaseConnection,
    ) -> Result<Self> {
        let config = Arc::new(config);
        let bootstrap_name = config
            .default_datasource_name()
            .unwrap_or("__bootstrap_primary")
            .to_string();
        let pool = DataSourcePool::build(config.clone(), bootstrap_name, bootstrap_connection)?;
        Self::with_pool(config, pool)
    }

    pub fn with_pool(config: Arc<ShardingConfig>, pool: DataSourcePool) -> Result<Self> {
        let key_registry = KeyGeneratorRegistry;
        let mut key_generators = BTreeMap::new();
        for rule in &config.sharding.tables {
            if let Some(key_generator) = &rule.key_generator {
                key_generators.insert(rule.logic_table.clone(), key_registry.build(key_generator)?);
            }
        }
        let lookup_index = Arc::new(LookupIndex::default());
        for definition in &config.sharding.lookup_indexes {
            lookup_index.register(LookupDefinition::from_config(definition));
        }
        let tenant_metadata = Arc::new(TenantMetadataStore::new());

        let inner = ShardingConnectionInner {
            router: Box::new(DefaultSqlRouter::new(config.clone(), lookup_index.clone())),
            rewriter: Box::new(DefaultSqlRewriter::new(config.clone())),
            executor: Box::new(ScatterGatherExecutor),
            merger: Box::new(DefaultResultMerger::new(config.clone())),
            key_generators,
            lookup_index,
            tenant_router: TenantRouter::new(config.clone(), tenant_metadata.clone()),
            shadow_router: ShadowRouter::new(config.clone()),
            tenant_metadata,
            metadata_loader: OnceLock::new(),
            auditor: Arc::new(DefaultSqlAuditor::default()),
            plugin_registry: OnceLock::new(),
            config,
            pool,
        };
        Ok(Self {
            inner: Arc::new(inner),
            hint_override: None,
            access_context_override: None,
            tenant_override: None,
            shadow_headers_override: None,
        })
    }

    pub fn key_generator(&self, logic_table: &str) -> Option<Arc<dyn KeyGenerator>> {
        self.inner.key_generators.get(logic_table).cloned()
    }

    /// 设置 SQL 改写插件注册表。
    /// 由 `SummerShardingPlugin::build()` 调用，将应用层注册的插件注入到连接中。
    pub fn set_plugin_registry(&self, registry: PluginRegistry) {
        if self.inner.plugin_registry.set(registry).is_err() {
            tracing::warn!("sql rewrite plugin registry was already initialized");
        }
    }

    pub fn set_metadata_loader(&self, loader: Arc<dyn TenantMetadataLoader>) {
        if self.inner.metadata_loader.set(loader).is_err() {
            tracing::warn!("tenant metadata loader was already initialized");
        }
    }

    /// 获取已注册插件的摘要信息（用于日志）
    pub fn plugin_summary(&self) -> String {
        self.inner
            .plugin_registry
            .get()
            .map(|r| r.summary())
            .unwrap_or_else(|| "none".to_string())
    }

    pub fn tenant_metadata_store(&self) -> Arc<TenantMetadataStore> {
        self.inner.tenant_metadata.clone()
    }

    pub fn datasource_names(&self) -> Vec<String> {
        self.inner.pool.datasource_names()
    }

    pub async fn health_check(&self) -> Vec<DataSourceHealth> {
        self.inner.pool.health_check().await
    }

    pub async fn refresh_route_states(&self) -> Vec<DataSourceRouteState> {
        self.inner.pool.refresh_read_write_route_states().await
    }

    pub fn route_states(&self) -> Vec<DataSourceRouteState> {
        self.inner
            .pool
            .primary_datasource_names()
            .into_iter()
            .filter_map(|primary| route_state(primary.as_str()))
            .collect()
    }

    pub fn auditor(&self) -> Arc<dyn SqlAuditor> {
        self.inner.auditor.clone()
    }

    pub fn lookup_index(&self) -> Arc<LookupIndex> {
        self.inner.lookup_index.clone()
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for ShardingConnection {
    fn get_database_backend(&self) -> DbBackend {
        self.inner
            .config
            .default_datasource_name()
            .and_then(|datasource| self.inner.pool.connection(datasource).ok())
            .map(|connection| connection.get_database_backend())
            .unwrap_or(DbBackend::Postgres)
    }

    async fn execute_raw(&self, stmt: Statement) -> std::result::Result<ExecResult, DbErr> {
        self.execute_with_raw(&self.inner.pool, stmt, false).await
    }

    async fn execute_unprepared(&self, sql: &str) -> std::result::Result<ExecResult, DbErr> {
        let stmt = Statement::from_string(self.get_database_backend(), sql);
        self.execute_raw(stmt).await
    }

    async fn query_one_raw(
        &self,
        stmt: Statement,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        self.query_one_with_raw(&self.inner.pool, stmt, false).await
    }

    async fn query_all_raw(&self, stmt: Statement) -> std::result::Result<Vec<QueryResult>, DbErr> {
        self.query_all_with_raw(&self.inner.pool, stmt, false).await
    }

    fn support_returning(&self) -> bool {
        self.inner
            .config
            .default_datasource_name()
            .and_then(|datasource| self.inner.pool.connection(datasource).ok())
            .is_some_and(|connection| connection.support_returning())
    }

    fn is_mock_connection(&self) -> bool {
        self.inner
            .config
            .default_datasource_name()
            .and_then(|datasource| self.inner.pool.connection(datasource).ok())
            .is_some_and(|connection| connection.is_mock_connection())
    }
}
#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use chrono::TimeZone;
    use futures::executor::block_on;
    use sea_orm::{
        ConnectionTrait, Database, DbBackend, MockDatabase, MockExecResult, Statement,
        TransactionSession,
    };

    use crate::{
        cdc::test_support::PrimaryReplicaTestCluster,
        config::{ShardingConfig, TenantIsolationLevel},
        connector::ShardingHint,
        datasource::{
            DataSourcePool, InMemoryRuntimeRecorder, clear_route_states, reset_runtime_recorder,
            set_runtime_recorder,
        },
        encrypt::{AesGcmEncryptor, EncryptAlgorithm},
        rewrite_plugin::{PluginRegistry, ShardingRouteInfo},
        tenant::{TenantContext, test_support},
    };
    use summer_sql_rewrite::{SqlRewriteContext, SqlRewritePlugin};

    use super::ShardingConnection;

    struct RouteCommentPlugin;

    impl SqlRewritePlugin for RouteCommentPlugin {
        fn name(&self) -> &str {
            "route_comment"
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut SqlRewriteContext) -> summer_sql_rewrite::Result<()> {
            let route = ctx
                .extension::<ShardingRouteInfo>()
                .expect("sharding route info extension");
            ctx.append_comment(format!("ds={}", route.datasource).as_str());
            Ok(())
        }
    }

    #[test]
    fn sharding_connection_routes_query_to_month_shards() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12
                "#,
            )
            .expect("config"),
        );

        let ai_connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                [BTreeMap::from([("id".to_string(), 1_i64.into())])],
                [BTreeMap::from([("id".to_string(), 2_i64.into())])],
            ])
            .into_connection();
        let log_connection = ai_connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), ai_connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT id FROM ai.log WHERE create_time >= $1 AND create_time < $2 ORDER BY create_time DESC LIMIT 10 OFFSET 20"#,
            [
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 4, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
            ],
        );

        let rows = block_on(sharding.query_all_raw(stmt)).expect("query");
        assert_eq!(rows.len(), 0);

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].statements()[0].sql.contains("ai.log_202602"));
        assert!(logs[1].statements()[0].sql.contains("ai.log_202603"));
        assert!(logs[0].statements()[0].sql.contains("LIMIT 30"));
    }

    #[test]
    fn sharding_connection_applies_hint_table_route() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool)
            .expect("connection")
            .with_hint(ShardingHint::Table("ai.log_202601".to_string()));

        block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM ai.log",
        )))
        .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].statements()[0].sql.contains("ai.log_202601"));
    }

    #[test]
    fn sharding_connection_exposes_bound_tenant_context() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_test]
                uri = "mock://test"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"
                "#,
            )
            .expect("config"),
        );
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([(
                "ds_test".to_string(),
                MockDatabase::new(DbBackend::Postgres).into_connection(),
            )]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        assert!(sharding.tenant_context().is_none());

        let tenant_bound = sharding.with_tenant_context(TenantContext::new(
            "T-BOUND",
            TenantIsolationLevel::SharedRow,
        ));

        let tenant = tenant_bound.tenant_context().expect("bound tenant");
        assert_eq!(tenant.tenant_id, "T-BOUND");
        assert_eq!(tenant.isolation_level, TenantIsolationLevel::SharedRow);
    }

    #[test]
    fn sharding_connection_allows_setting_plugin_registry_after_connection_is_cloned() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_test]
                uri = "mock://test"
                schema = "test"
                role = "primary"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_test".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");
        let cloned = sharding.clone();

        let mut registry = PluginRegistry::new();
        registry.register(RouteCommentPlugin);
        sharding.set_plugin_registry(registry);

        block_on(cloned.query_all_raw(Statement::from_string(DbBackend::Postgres, "SELECT 1")))
            .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].statements()[0].sql.contains("ds="));
        assert!(sharding.plugin_summary().contains("route_comment"));
        assert!(cloned.plugin_summary().contains("route_comment"));
    }

    #[test]
    fn sharding_connection_injects_tenant_filter_for_shared_row() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        block_on(
            sharding
                .with_tenant_context(TenantContext::new("T-001", TenantIsolationLevel::SharedRow))
                .query_all_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "SELECT id FROM ai.log WHERE status = 1",
                )),
        )
        .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].statements()[0].sql.contains("tenant_id = 'T-001'"));
    }

    #[test]
    fn sharding_connection_uses_metadata_isolation_for_tenant_rewrite() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");
        sharding
            .tenant_metadata_store()
            .upsert(crate::tenant::TenantMetadataRecord {
                tenant_id: "T-ENT-01".to_string(),
                isolation_level: TenantIsolationLevel::SeparateSchema,
                status: Some("active".to_string()),
                schema_name: Some("tenant_ent01".to_string()),
                datasource_name: None,
                db_uri: None,
                db_enable_logging: None,
                db_min_conns: None,
                db_max_conns: None,
                db_connect_timeout_ms: None,
                db_idle_timeout_ms: None,
                db_acquire_timeout_ms: None,
                db_test_before_acquire: None,
            });

        block_on(
            sharding
                .with_tenant_context(TenantContext::new(
                    "T-ENT-01",
                    TenantIsolationLevel::SharedRow,
                ))
                .query_all_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "SELECT id FROM ai.log WHERE status = 1",
                )),
        )
        .expect("query");

        let logs = log_connection.into_transaction_log();
        let sql = logs[0].statements()[0].sql.clone();
        assert_eq!(logs.len(), 1);
        assert!(sql.contains("tenant_ent01.log"), "sql={sql}");
        assert!(!sql.contains("tenant_id = 'T-ENT-01'"), "sql={sql}");
    }

    #[test]
    fn sharding_connection_rejects_inactive_tenant_metadata() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");
        sharding
            .tenant_metadata_store()
            .upsert(crate::tenant::TenantMetadataRecord {
                tenant_id: "T-INACTIVE".to_string(),
                isolation_level: TenantIsolationLevel::SharedRow,
                status: Some("inactive".to_string()),
                schema_name: None,
                datasource_name: None,
                db_uri: None,
                db_enable_logging: None,
                db_min_conns: None,
                db_max_conns: None,
                db_connect_timeout_ms: None,
                db_idle_timeout_ms: None,
                db_acquire_timeout_ms: None,
                db_test_before_acquire: None,
            });

        let error = block_on(
            sharding
                .with_tenant_context(TenantContext::new(
                    "T-INACTIVE",
                    TenantIsolationLevel::SharedRow,
                ))
                .query_all_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "SELECT id FROM ai.log WHERE status = 1",
                )),
        )
        .expect_err("inactive tenant should be rejected");

        assert!(error.to_string().contains("not active"));
    }

    #[tokio::test]
    async fn sharding_connection_exposes_health_and_route_state_snapshots() {
        clear_route_states();
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_primary]
                uri = "mock://primary"
                schema = "test"
                role = "primary"

                [datasources.ds_replica]
                uri = "mock://replica"
                schema = "test"
                role = "replica"

                [read_write_splitting]
                enabled = true

                [[read_write_splitting.rules]]
                name = "rw"
                primary = "ds_primary"
                replicas = ["ds_replica"]
                load_balance = "round_robin"
                "#,
            )
            .expect("config"),
        );
        let primary = MockDatabase::new(DbBackend::Postgres).into_connection();
        let replica = MockDatabase::new(DbBackend::Postgres).into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary),
                ("ds_replica".to_string(), replica),
            ]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let health = sharding.health_check().await;
        assert_eq!(health.len(), 2);
        assert!(health.iter().all(|item| item.reachable));

        let refreshed = sharding.refresh_route_states().await;
        assert_eq!(refreshed.len(), 1);
        assert_eq!(refreshed[0].configured_primary, "ds_primary");

        let snapshot = sharding.route_states();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].rule_name, "rw");
        assert_eq!(snapshot[0].configured_primary, "ds_primary");

        clear_route_states();
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL seed tenant metadata data"]
    async fn sharding_connection_routes_using_real_tenant_metadata_from_database() {
        let database_url = test_support::e2e_database_url();
        let replica_url = test_support::e2e_replica_database_url();
        test_support::prepare_probe_e2e_environment(&database_url, &replica_url)
            .await
            .expect("prepare probe environment");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{database_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        let sharding = ShardingConnection::build(config, metadata_connection.clone())
            .await
            .expect("build sharding connection");
        crate::tenant::test_support::register_test_metadata_loader(&sharding);

        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let rows = sharding
            .with_tenant_context(TenantContext::new(
                "T-SEED-SCHEMA",
                TenantIsolationLevel::SharedRow,
            ))
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.tenant_probe_isolated ORDER BY id",
            ))
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        let payload: String = rows[0].try_get("", "payload").expect("payload");
        assert_eq!(payload, "schema-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-schema seed tenant metadata data"]
    async fn sharding_connection_routes_isolated_probe_to_separate_schema() {
        let database_url = test_support::e2e_database_url();
        let replica_url = test_support::e2e_replica_database_url();
        test_support::prepare_probe_e2e_environment(&database_url, &replica_url)
            .await
            .expect("prepare probe environment");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{database_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        let sharding = ShardingConnection::build(config, metadata_connection.clone())
            .await
            .expect("build sharding connection");
        crate::tenant::test_support::register_test_metadata_loader(&sharding);

        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let rows = sharding
            .with_tenant_context(TenantContext::new(
                "T-SEED-SCHEMA",
                TenantIsolationLevel::SharedRow,
            ))
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.tenant_probe_isolated ORDER BY id",
            ))
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        let payload: String = rows[0].try_get("", "payload").expect("payload");
        assert_eq!(payload, "schema-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-table seed tenant metadata data"]
    async fn sharding_connection_routes_isolated_probe_to_separate_table() {
        let database_url = test_support::e2e_database_url();
        let replica_url = test_support::e2e_replica_database_url();
        test_support::prepare_probe_e2e_environment(&database_url, &replica_url)
            .await
            .expect("prepare probe environment");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{database_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        let sharding = ShardingConnection::build(config, metadata_connection.clone())
            .await
            .expect("build sharding connection");
        crate::tenant::test_support::register_test_metadata_loader(&sharding);

        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let rows = sharding
            .with_tenant_context(TenantContext::new(
                "T-SEED-TABLE",
                TenantIsolationLevel::SharedRow,
            ))
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.tenant_probe_isolated ORDER BY id",
            ))
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        let payload: String = rows[0].try_get("", "payload").expect("payload");
        assert_eq!(payload, "table-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-database seed tenant metadata data"]
    async fn sharding_connection_routes_isolated_probe_to_separate_database() {
        let database_url = test_support::e2e_database_url();
        let replica_url = test_support::e2e_replica_database_url();
        test_support::prepare_probe_e2e_environment(&database_url, &replica_url)
            .await
            .expect("prepare probe environment");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{database_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        let sharding = ShardingConnection::build(config, metadata_connection.clone())
            .await
            .expect("build sharding connection");
        crate::tenant::test_support::register_test_metadata_loader(&sharding);

        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let rows = sharding
            .with_tenant_context(TenantContext::new(
                "T-SEED-DB",
                TenantIsolationLevel::SharedRow,
            ))
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.tenant_probe_isolated ORDER BY id",
            ))
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        let payload: String = rows[0].try_get("", "payload").expect("payload");
        assert_eq!(payload, "db-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL primary and replica seed data"]
    async fn sharding_connection_routes_reads_to_replica_and_transaction_reads_to_primary() {
        let primary_url = test_support::e2e_database_url();
        let replica_url = test_support::e2e_replica_database_url();
        test_support::prepare_rw_probe_environment(&primary_url, &replica_url)
            .await
            .expect("prepare rw probe");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_primary]
                uri = "{primary_url}"
                schema = "test"
                role = "primary"

                [datasources.ds_replica]
                uri = "{replica_url}"
                schema = "test"
                role = "replica"

                [read_write_splitting]
                enabled = true

                [[read_write_splitting.rules]]
                name = "rw"
                primary = "ds_primary"
                replicas = ["ds_replica"]
                load_balance = "round_robin"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let primary_connection = Database::connect(&primary_url)
            .await
            .expect("connect primary");
        let replica_connection = Database::connect(&replica_url)
            .await
            .expect("connect replica");
        let config = Arc::new(config);
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary_connection),
                ("ds_replica".to_string(), replica_connection),
            ]),
        )
        .expect("pool");
        let sharding =
            ShardingConnection::with_pool(config, pool).expect("build sharding connection");

        let replica_rows = sharding
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT payload FROM test.rw_probe ORDER BY id",
            ))
            .await
            .expect("replica query");
        let replica_payload: String = replica_rows[0].try_get("", "payload").expect("payload");
        assert_eq!(replica_payload, "replica-read");

        let tx = sea_orm::TransactionTrait::begin(&sharding)
            .await
            .expect("transaction");
        let primary_rows = tx
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT payload FROM test.rw_probe ORDER BY id",
            ))
            .await
            .expect("primary query in tx");
        let primary_payload: String = primary_rows[0].try_get("", "payload").expect("payload");
        assert_eq!(primary_payload, "primary-read");
        tx.commit().await.expect("commit");
    }

    #[tokio::test]
    #[ignore = "requires docker"]
    async fn sharding_connection_routes_writes_to_promoted_replica_after_real_failover() {
        let cluster = PrimaryReplicaTestCluster::start()
            .await
            .expect("start primary replica cluster");
        cluster.seed_rw_probe().await.expect("seed rw probe");

        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_primary]
                uri = "{}"
                schema = "test"
                role = "primary"

                [datasources.ds_replica]
                uri = "{}"
                schema = "test"
                role = "replica"

                [read_write_splitting]
                enabled = true

                [[read_write_splitting.rules]]
                name = "rw"
                primary = "ds_primary"
                replicas = ["ds_replica"]
                load_balance = "round_robin"
                "#,
                cluster.primary_database_url(),
                cluster.replica_database_url(),
            )
            .as_str(),
        )
        .expect("config");
        let primary_connection = Database::connect(cluster.primary_database_url())
            .await
            .expect("connect primary");
        let replica_connection = Database::connect(cluster.replica_database_url())
            .await
            .expect("connect replica");
        let config = Arc::new(config);
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary_connection),
                ("ds_replica".to_string(), replica_connection),
            ]),
        )
        .expect("pool");
        let sharding =
            ShardingConnection::with_pool(config, pool).expect("build sharding connection");

        let initial_states = sharding.inner.pool.refresh_read_write_route_states().await;
        assert_eq!(
            initial_states[0].write_target.as_deref(),
            Some("ds_primary")
        );
        assert!(!initial_states[0].failover_active);

        let rows = sharding
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.rw_failover_probe ORDER BY id",
            ))
            .await
            .expect("initial read query");
        assert_eq!(rows.len(), 1);

        cluster
            .promote_replica_and_stop_primary()
            .await
            .expect("promote replica");

        let failover_states = sharding.inner.pool.refresh_read_write_route_states().await;
        assert_eq!(
            failover_states[0].write_target.as_deref(),
            Some("ds_replica")
        );
        assert!(failover_states[0].failover_active);

        sharding
            .execute_unprepared(
                "INSERT INTO test.rw_failover_probe(id, payload) VALUES (2, 'post-failover-write')",
            )
            .await
            .expect("write should route to promoted replica");

        let replica = Database::connect(cluster.replica_database_url())
            .await
            .expect("connect promoted replica");
        let failover_rows = replica
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, payload FROM test.rw_failover_probe ORDER BY id",
            ))
            .await
            .expect("query promoted replica");
        assert_eq!(failover_rows.len(), 2);
        let second_payload: String = failover_rows[1].try_get("", "payload").expect("payload");
        assert_eq!(second_payload, "post-failover-write");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL shadow seed data"]
    async fn sharding_connection_routes_real_shadow_hint_to_shadow_table() {
        let database_url = test_support::e2e_database_url();
        test_support::prepare_shadow_probe_environment(&database_url)
            .await
            .expect("prepare shadow probe");
        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{database_url}"
                schema = "test"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [shadow.table_mode]
                  enabled = true
                  tables = ["test.shadow_probe"]
                "#
            )
            .as_str(),
        )
        .expect("config");

        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        let normal = ShardingConnection::build(config.clone(), metadata_connection.clone())
            .await
            .expect("build normal connection");
        let shadow = ShardingConnection::build(config, metadata_connection)
            .await
            .expect("build shadow connection")
            .with_hint(ShardingHint::Shadow);

        let base_rows = normal
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT payload FROM test.shadow_probe ORDER BY id",
            ))
            .await
            .expect("base query");
        let base_payload: String = base_rows[0].try_get("", "payload").expect("payload");
        assert_eq!(base_payload, "base-row");

        let shadow_rows = shadow
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT payload FROM test.shadow_probe ORDER BY id",
            ))
            .await
            .expect("shadow query");
        let shadow_payload: String = shadow_rows[0].try_get("", "payload").expect("payload");
        assert_eq!(shadow_payload, "shadow-row");
    }

    #[test]
    fn sharding_connection_rewrites_binding_tables_together() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.request"
                actual_tables = "ai.request_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                [[sharding.tables]]
                logic_table = "ai.request_execution"
                actual_tables = "ai.request_execution_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                [[sharding.binding_groups]]
                tables = ["ai.request", "ai.request_execution"]
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT r.id, e.status
               FROM ai.request r
               JOIN ai.request_execution e ON r.id = e.request_id
               WHERE r.create_time >= $1 AND r.create_time < $2"#,
            [
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 3, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 4, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
            ],
        );

        block_on(sharding.query_all_raw(stmt)).expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        let sql = &logs[0].statements()[0].sql;
        assert!(sql.contains("ai.request_202603"));
        assert!(sql.contains("ai.request_execution_202603"));
    }

    #[test]
    fn sharding_connection_routes_non_sharding_key_query_via_lookup_table() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([(
                "create_time".to_string(),
                "2026-03-15T00:00:00+00:00".into(),
            )])]])
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM ai.log WHERE trace_id = 'tr-001'",
        )))
        .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 2);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("ai.log_lookup_trace_id")
        );
        assert!(logs[1].statements()[0].sql.contains("ai.log_202603"));
    }

    #[test]
    fn sharding_connection_syncs_lookup_table_on_insert() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "INSERT INTO ai.log (trace_id, create_time) VALUES ('tr-001', '2026-03-15T00:00:00+00:00')",
        )))
        .expect("insert");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 2);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("INSERT INTO ai.log_202603")
        );
        assert!(
            logs[1].statements()[0]
                .sql
                .contains("INSERT INTO ai.log_lookup_trace_id")
        );
    }

    #[test]
    fn sharding_connection_rejects_sharding_key_update() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([(
                "create_time".to_string(),
                "2026-03-15T00:00:00+00:00".into(),
            )])]])
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let error = block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "UPDATE ai.log SET create_time = '2026-03-18T00:00:00+00:00' WHERE trace_id = 'tr-001'",
        )))
        .expect_err("sharding key update should be rejected");

        let logs = log_connection.into_transaction_log();
        assert!(
            error
                .to_string()
                .contains("updating sharding column `create_time`")
        );
        assert_eq!(logs.len(), 1);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("ai.log_lookup_trace_id")
        );
    }

    #[test]
    fn sharding_connection_rejects_expression_based_sharding_key_update() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([(
                "create_time".to_string(),
                "2026-03-15T00:00:00+00:00".into(),
            )])]])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let error = block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "UPDATE ai.log SET create_time = now() WHERE trace_id = 'tr-001'",
        )))
        .expect_err("sharding key expression update should be rejected");

        assert!(
            error
                .to_string()
                .contains("updating sharding column `create_time`")
        );
        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
    }

    #[test]
    fn sharding_connection_syncs_lookup_table_on_delete() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([(
                "create_time".to_string(),
                "2026-03-15T00:00:00+00:00".into(),
            )])]])
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "DELETE FROM ai.log WHERE trace_id = 'tr-001'",
        )))
        .expect("delete");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 3);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("ai.log_lookup_trace_id")
        );
        assert!(
            logs[1].statements()[0]
                .sql
                .contains("DELETE FROM ai.log_202603")
        );
        assert!(
            logs[2].statements()[0]
                .sql
                .contains("DELETE FROM ai.log_lookup_trace_id")
        );
    }

    #[test]
    fn sharding_connection_rejects_lookup_mutation_without_exact_lookup_predicate() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres).into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let error = block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "DELETE FROM ai.log WHERE create_time = '2026-03-15T00:00:00+00:00'",
        )))
        .expect_err("ambiguous lookup delete should be rejected");

        assert!(
            error
                .to_string()
                .contains("requires an exact `trace_id` predicate")
        );
        assert!(log_connection.into_transaction_log().is_empty());
    }

    #[test]
    fn sharding_connection_rejects_lookup_update_without_exact_lookup_predicate() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [[sharding.lookup_indexes]]
                logic_table = "ai.log"
                lookup_column = "trace_id"
                lookup_table = "ai.log_lookup_trace_id"
                sharding_column = "create_time"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres).into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let error = block_on(sharding.execute_raw(Statement::from_string(
            DbBackend::Postgres,
            "UPDATE ai.log SET payload = 'x' WHERE create_time = '2026-03-15T00:00:00+00:00'",
        )))
        .expect_err("ambiguous lookup update should be rejected");

        assert!(
            error
                .to_string()
                .contains("requires an exact `trace_id` predicate")
        );
        assert!(log_connection.into_transaction_log().is_empty());
    }

    #[test]
    fn sharding_connection_records_query_fanout_and_slow_query_metrics() {
        let recorder = std::sync::Arc::new(InMemoryRuntimeRecorder::default());
        set_runtime_recorder(recorder.clone());

        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.log"
                actual_tables = "ai.log_${yyyyMM}"
                sharding_column = "create_time"
                algorithm = "time_range"

                  [sharding.tables.algorithm_props]
                  granularity = "month"
                  retention_months = 12

                [audit]
                enabled = true
                slow_query_threshold_ms = 0
                log_full_scatter = true
                "#,
            )
            .expect("config"),
        );

        let ai_connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                Vec::<BTreeMap<String, sea_orm::Value>>::new(),
                Vec::<BTreeMap<String, sea_orm::Value>>::new(),
            ])
            .into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), ai_connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT id FROM ai.log WHERE create_time >= $1 AND create_time < $2 ORDER BY create_time DESC LIMIT 10 OFFSET 20"#,
            [
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 4, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
            ],
        );

        block_on(sharding.query_all_raw(stmt)).expect("query");

        let snapshot = recorder.snapshot();
        assert!(
            snapshot.fanouts.iter().any(|metric| metric.fanout >= 2),
            "fanouts={:?}",
            snapshot.fanouts
        );
        assert!(
            !snapshot.slow_queries.is_empty(),
            "slow_queries={:?}",
            snapshot.slow_queries
        );

        reset_runtime_recorder();
    }

    #[test]
    fn sharding_connection_routes_shadow_hint_to_shadow_table() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [shadow.table_mode]
                  enabled = true
                  tables = ["ai.log"]
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool)
            .expect("connection")
            .with_hint(ShardingHint::Shadow);

        block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM ai.log",
        )))
        .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].statements()[0].sql.contains("ai.log_shadow"));
    }

    #[test]
    fn sharding_connection_routes_shadow_header_to_shadow_table() {
        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [shadow.table_mode]
                  enabled = true
                  tables = ["ai.log"]

                  [[shadow.conditions]]
                  type = "header"
                  key = "X-Shadow"
                  value = "true"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool)
            .expect("connection")
            .with_shadow_headers(BTreeMap::from([(
                "X-Shadow".to_string(),
                "true".to_string(),
            )]));

        block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM ai.log",
        )))
        .expect("query");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].statements()[0].sql.contains("ai.log_shadow"));
    }

    #[test]
    fn sharding_connection_decrypts_and_masks_sensitive_columns() {
        unsafe {
            std::env::set_var(
                "SUMMER_SHARDING_TEST_AES",
                "12345678901234567890123456789012",
            );
        }
        let cipher = AesGcmEncryptor::from_material(b"12345678901234567890123456789012")
            .expect("encryptor")
            .encrypt("13812341234")
            .expect("cipher");

        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_sys]
                uri = "mock://sys"
                schema = "sys"
                role = "primary"

                [encrypt]
                enabled = true

                  [[encrypt.rules]]
                  table = "sys.user"
                  column = "phone"
                  cipher_column = "phone_cipher"
                  algorithm = "aes"
                  key_env = "SUMMER_SHARDING_TEST_AES"

                [masking]
                enabled = true

                  [[masking.rules]]
                  table = "sys.user"
                  column = "phone"
                  algorithm = "phone"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([(
                "phone".to_string(),
                cipher.clone().into(),
            )])]])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_sys".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let rows = block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT phone FROM sys.user WHERE id = 1",
        )))
        .expect("query");

        assert_eq!(
            rows[0].try_get::<String>("", "phone").expect("phone"),
            "138****1234"
        );
        let logs = log_connection.into_transaction_log();
        assert!(logs[0].statements()[0].sql.contains("phone_cipher"));
    }

    #[test]
    fn sharding_connection_skip_masking_returns_plaintext() {
        unsafe {
            std::env::set_var(
                "SUMMER_SHARDING_TEST_AES",
                "12345678901234567890123456789012",
            );
        }
        let cipher = AesGcmEncryptor::from_material(b"12345678901234567890123456789012")
            .expect("encryptor")
            .encrypt("13812341234")
            .expect("cipher");

        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_sys]
                uri = "mock://sys"
                schema = "sys"
                role = "primary"

                [encrypt]
                enabled = true

                  [[encrypt.rules]]
                  table = "sys.user"
                  column = "phone"
                  cipher_column = "phone_cipher"
                  algorithm = "aes"
                  key_env = "SUMMER_SHARDING_TEST_AES"

                [masking]
                enabled = true

                  [[masking.rules]]
                  table = "sys.user"
                  column = "phone"
                  algorithm = "phone"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([("phone".to_string(), cipher.into())])]])
            .into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_sys".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool)
            .expect("connection")
            .with_hint(ShardingHint::SkipMasking);

        let rows = block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT phone FROM sys.user WHERE id = 1",
        )))
        .expect("query");

        assert_eq!(
            rows[0].try_get::<String>("", "phone").expect("phone"),
            "13812341234"
        );
    }

    #[test]
    fn sharding_connection_access_context_skip_masking_returns_plaintext() {
        unsafe {
            std::env::set_var(
                "SUMMER_SHARDING_TEST_AES",
                "12345678901234567890123456789012",
            );
        }
        let cipher = AesGcmEncryptor::from_material(b"12345678901234567890123456789012")
            .expect("encryptor")
            .encrypt("13812341234")
            .expect("cipher");

        let config = std::sync::Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_sys]
                uri = "mock://sys"
                schema = "sys"
                role = "primary"

                [encrypt]
                enabled = true

                  [[encrypt.rules]]
                  table = "sys.user"
                  column = "phone"
                  cipher_column = "phone_cipher"
                  algorithm = "aes"
                  key_env = "SUMMER_SHARDING_TEST_AES"

                [masking]
                enabled = true

                  [[masking.rules]]
                  table = "sys.user"
                  column = "phone"
                  algorithm = "phone"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([("phone".to_string(), cipher.into())])]])
            .into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_sys".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool)
            .expect("connection")
            .with_access_context(
                crate::connector::hint::ShardingAccessContext::default()
                    .with_permission("masking:skip"),
            );

        let rows = block_on(sharding.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT phone FROM sys.user WHERE id = 1",
        )))
        .expect("query");

        assert_eq!(
            rows[0].try_get::<String>("", "phone").expect("phone"),
            "13812341234"
        );
    }
}
