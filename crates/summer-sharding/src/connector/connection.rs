use std::sync::{Arc, OnceLock};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Statement,
};

use crate::{
    config::ShardingConfig,
    connector::{ShardingAccessContext, ShardingHint},
    datasource::{DataSourceHealth, DataSourcePool, DataSourceRouteState, route_state},
    error::Result,
    execute::{Executor, ScatterGatherExecutor},
    merge::{DefaultResultMerger, ResultMerger},
    rewrite::{DefaultSqlRewriter, SqlRewriter},
    rewrite_plugin::PluginRegistry,
    router::{DefaultSqlRouter, SqlRouter},
    tenant::{TenantMetadataLoader, TenantMetadataStore, TenantRouter},
};

mod audit;
mod exec;
mod metadata;
mod overrides;

#[derive(Clone)]
pub struct ShardingConnection {
    pub(crate) inner: Arc<ShardingConnectionInner>,
    pub(crate) hint_override: Option<ShardingHint>,
    pub(crate) access_context_override: Option<ShardingAccessContext>,
    pub(crate) tenant_override: Option<crate::tenant::TenantContext>,
}

#[derive(Clone, Default)]
pub(crate) struct ExecutionOverrides {
    pub(crate) hint: Option<ShardingHint>,
    pub(crate) access_context: Option<ShardingAccessContext>,
    pub(crate) tenant: Option<crate::tenant::TenantContext>,
}

pub(crate) struct ShardingConnectionInner {
    pub(crate) config: Arc<ShardingConfig>,
    pub(crate) pool: DataSourcePool,
    pub(crate) router: Box<dyn SqlRouter>,
    pub(crate) rewriter: Box<dyn SqlRewriter>,
    pub(crate) executor: Box<dyn Executor>,
    pub(crate) merger: Box<dyn ResultMerger>,
    pub(crate) tenant_metadata: Arc<TenantMetadataStore>,
    pub(crate) metadata_loader: OnceLock<Arc<dyn TenantMetadataLoader>>,
    pub(crate) tenant_router: TenantRouter,
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
        let tenant_metadata = Arc::new(TenantMetadataStore::new());

        let inner = ShardingConnectionInner {
            router: Box::new(DefaultSqlRouter::new(config.clone())),
            rewriter: Box::new(DefaultSqlRewriter::new(config.clone())),
            executor: Box::new(ScatterGatherExecutor),
            merger: Box::new(DefaultResultMerger::new(config.clone())),
            tenant_router: TenantRouter::new(config.clone(), tenant_metadata.clone()),
            tenant_metadata,
            metadata_loader: OnceLock::new(),
            plugin_registry: OnceLock::new(),
            config,
            pool,
        };
        Ok(Self {
            inner: Arc::new(inner),
            hint_override: None,
            access_context_override: None,
            tenant_override: None,
        })
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
    use std::collections::BTreeMap;

    use chrono::TimeZone;
    use futures::executor::block_on;
    use sea_orm::{ConnectionTrait, DbBackend, MockDatabase, Statement};

    use crate::sql_rewrite::{SqlRewriteContext, SqlRewritePlugin};
    use crate::{
        config::{ShardingConfig, TenantIsolationLevel},
        connector::ShardingHint,
        datasource::{
            DataSourcePool, InMemoryRuntimeRecorder, clear_route_states, reset_runtime_recorder,
            set_runtime_recorder,
        },
        rewrite_plugin::{PluginRegistry, ShardingRouteInfo},
        tenant::TenantContext,
    };

    use super::ShardingConnection;

    struct RouteCommentPlugin;

    impl SqlRewritePlugin for RouteCommentPlugin {
        fn name(&self) -> &str {
            "route_comment"
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut SqlRewriteContext) -> crate::sql_rewrite::Result<()> {
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
}
