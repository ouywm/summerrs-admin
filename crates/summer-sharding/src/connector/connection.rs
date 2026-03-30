use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use sea_orm::{ConnectionTrait, DbBackend, DbErr, ExecResult, QueryResult, Statement, Values};

use crate::{
    audit::{AuditEvent, DefaultSqlAuditor, SqlAuditor},
    config::ShardingConfig,
    connector::statement::{StatementContext, analyze_statement},
    connector::{ShardingAccessContext, ShardingHint},
    datasource::{
        DataSourceHealth, DataSourcePool, DataSourceRouteState, FanoutMetric, SlowQueryMetric,
        record_fanout, record_slow_query, route_state,
    },
    error::{Result, ShardingError},
    execute::{ExecutionUnit, Executor, RawStatementExecutor, ScatterGatherExecutor},
    keygen::{KeyGenerator, KeyGeneratorRegistry},
    lookup::{
        LookupDefinition, LookupIndex, normalize_column, query_result_to_sharding_value,
        sharding_value_to_sea_value, split_qualified_name, update_assignment_value,
        update_assigns_column,
    },
    merge::{DefaultResultMerger, ResultMerger},
    rewrite::{DefaultSqlRewriter, SqlRewriter},
    rewrite_plugin::PluginRegistry,
    router::{DefaultSqlRouter, RoutePlan, SqlOperation, SqlRouter},
    shadow::ShadowRouter,
    tenant::{TenantMetadataStore, TenantRouter},
};

#[derive(Clone)]
pub struct ShardingConnection {
    pub(crate) inner: Arc<ShardingConnectionInner>,
    pub(crate) hint_override: Option<ShardingHint>,
    pub(crate) access_context_override: Option<ShardingAccessContext>,
    pub(crate) tenant_override: Option<crate::tenant::TenantContext>,
    pub(crate) shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
}

pub(crate) struct ShardingConnectionInner {
    pub(crate) config: Arc<ShardingConfig>,
    pub(crate) pool: DataSourcePool,
    pub(crate) router: Arc<dyn SqlRouter>,
    pub(crate) rewriter: Arc<dyn SqlRewriter>,
    pub(crate) executor: Arc<dyn Executor>,
    pub(crate) merger: Arc<dyn ResultMerger>,
    pub(crate) key_generators: BTreeMap<String, Arc<dyn KeyGenerator>>,
    pub(crate) lookup_index: Arc<LookupIndex>,
    pub(crate) tenant_metadata: Arc<TenantMetadataStore>,
    pub(crate) tenant_router: TenantRouter,
    pub(crate) shadow_router: ShadowRouter,
    pub(crate) auditor: Arc<dyn SqlAuditor>,
    /// SQL 改写插件注册表（可选，由应用层通过 ShardingRewriteConfigurator 注入）
    pub(crate) plugin_registry: Option<Arc<PluginRegistry>>,
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
    pub async fn build(config: ShardingConfig) -> Result<Self> {
        let config = Arc::new(config);
        let pool = DataSourcePool::build(config.clone()).await?;
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
        let tenant_metadata = TenantMetadataStore::new();

        let inner = ShardingConnectionInner {
            router: Arc::new(DefaultSqlRouter::new(config.clone(), lookup_index.clone())),
            rewriter: Arc::new(DefaultSqlRewriter::new(config.clone())),
            executor: Arc::new(ScatterGatherExecutor),
            merger: Arc::new(DefaultResultMerger::new(config.clone())),
            key_generators,
            lookup_index,
            tenant_router: TenantRouter::new(config.clone(), tenant_metadata.clone()),
            shadow_router: ShadowRouter::new(config.clone()),
            tenant_metadata,
            auditor: Arc::new(DefaultSqlAuditor::default()),
            plugin_registry: None,
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
    ///
    /// # Panics
    /// 如果 `ShardingConnection` 的内部 `Arc` 已被克隆（强引用计数 > 1），则 panic。
    /// 请确保在连接被共享之前调用此方法。
    pub fn set_plugin_registry(&mut self, registry: PluginRegistry) {
        let inner = Arc::get_mut(&mut self.inner)
            .expect("set_plugin_registry must be called before the connection is shared");
        inner.plugin_registry = Some(Arc::new(registry));
    }

    /// 获取已注册插件的摘要信息（用于日志）
    pub fn plugin_summary(&self) -> String {
        self.inner
            .plugin_registry
            .as_ref()
            .map(|r| r.summary())
            .unwrap_or_else(|| "none".to_string())
    }

    pub fn with_hint(&self, hint: ShardingHint) -> Self {
        Self {
            inner: self.inner.clone(),
            hint_override: Some(hint),
            access_context_override: self.access_context_override.clone(),
            tenant_override: self.tenant_override.clone(),
            shadow_headers_override: self.shadow_headers_override.clone(),
        }
    }

    pub fn with_tenant_context(&self, tenant: crate::tenant::TenantContext) -> Self {
        Self {
            inner: self.inner.clone(),
            hint_override: self.hint_override.clone(),
            access_context_override: self.access_context_override.clone(),
            tenant_override: Some(self.resolve_tenant_context(tenant)),
            shadow_headers_override: self.shadow_headers_override.clone(),
        }
    }

    pub fn with_access_context(&self, context: ShardingAccessContext) -> Self {
        Self {
            inner: self.inner.clone(),
            hint_override: self.hint_override.clone(),
            access_context_override: Some(context),
            tenant_override: self.tenant_override.clone(),
            shadow_headers_override: self.shadow_headers_override.clone(),
        }
    }

    pub fn with_shadow_headers(&self, headers: BTreeMap<String, String>) -> Self {
        Self {
            inner: self.inner.clone(),
            hint_override: self.hint_override.clone(),
            access_context_override: self.access_context_override.clone(),
            tenant_override: self.tenant_override.clone(),
            shadow_headers_override: Some(Arc::new(headers)),
        }
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

    pub fn resolve_tenant_context(
        &self,
        tenant: crate::tenant::TenantContext,
    ) -> crate::tenant::TenantContext {
        self.inner.tenant_router.resolve_context(tenant)
    }

    pub fn auditor(&self) -> Arc<dyn SqlAuditor> {
        self.inner.auditor.clone()
    }

    pub fn lookup_index(&self) -> Arc<LookupIndex> {
        self.inner.lookup_index.clone()
    }

    pub async fn reload_tenant_metadata(
        &self,
        metadata_connection: &sea_orm::DatabaseConnection,
    ) -> Result<()> {
        self.inner
            .tenant_metadata
            .refresh_from_connection(metadata_connection)
            .await?;
        self.inner
            .pool
            .sync_tenant_datasources(self.inner.tenant_metadata.as_ref())
            .await?;
        Ok(())
    }

    pub async fn apply_tenant_metadata_notification(
        &self,
        metadata_connection: &sea_orm::DatabaseConnection,
        payload: &str,
    ) -> Result<()> {
        let outcome = self
            .inner
            .tenant_metadata
            .apply_notification_payload(payload)?;
        if outcome == crate::tenant::TenantMetadataApplyOutcome::ReloadRequired {
            self.reload_tenant_metadata(metadata_connection).await?;
        } else {
            self.inner
                .pool
                .sync_tenant_datasources(self.inner.tenant_metadata.as_ref())
                .await?;
        }
        Ok(())
    }

    pub fn spawn_tenant_metadata_polling(
        &self,
        metadata_connection: sea_orm::DatabaseConnection,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let connection = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = connection
                    .reload_tenant_metadata(&metadata_connection)
                    .await;
            }
        })
    }

    #[allow(dead_code)]
    pub(crate) async fn prepare_statement(
        &self,
        stmt: &Statement,
        force_primary: bool,
    ) -> Result<(StatementContext, RoutePlan, Vec<ExecutionUnit>)> {
        self.inner
            .prepare_statement(
                &self.inner.pool,
                stmt,
                force_primary,
                self.hint_override.clone(),
                self.access_context_override.clone(),
                self.tenant_override.clone(),
                self.shadow_headers_override.clone(),
            )
            .await
    }

    pub(crate) async fn execute_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
    ) -> std::result::Result<ExecResult, DbErr> {
        self.inner
            .execute_with_raw(
                raw,
                stmt,
                force_primary,
                self.hint_override.clone(),
                self.access_context_override.clone(),
                self.tenant_override.clone(),
                self.shadow_headers_override.clone(),
            )
            .await
    }

    pub(crate) async fn query_one_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        self.inner
            .query_one_with_raw(
                raw,
                stmt,
                force_primary,
                self.hint_override.clone(),
                self.access_context_override.clone(),
                self.tenant_override.clone(),
                self.shadow_headers_override.clone(),
            )
            .await
    }

    pub(crate) async fn query_all_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        self.inner
            .query_all_with_raw(
                raw,
                stmt,
                force_primary,
                self.hint_override.clone(),
                self.access_context_override.clone(),
                self.tenant_override.clone(),
                self.shadow_headers_override.clone(),
            )
            .await
    }
}

impl ShardingConnectionInner {
    fn ensure_tenant_available(&self, tenant: Option<&crate::tenant::TenantContext>) -> Result<()> {
        let Some(tenant) = tenant else {
            return Ok(());
        };
        let Some(metadata) = self.tenant_metadata.get(tenant.tenant_id.as_str()) else {
            return Ok(());
        };
        match metadata.status.as_deref() {
            None => Ok(()),
            Some(status) if status.eq_ignore_ascii_case("active") => Ok(()),
            Some(status) => Err(ShardingError::Route(format!(
                "tenant `{}` is not active: {status}",
                tenant.tenant_id
            ))),
        }
    }

    pub(crate) async fn prepare_statement(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: &Statement,
        force_primary: bool,
        hint_override: Option<ShardingHint>,
        access_context_override: Option<ShardingAccessContext>,
        tenant_override: Option<crate::tenant::TenantContext>,
        shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
    ) -> Result<(StatementContext, RoutePlan, Vec<ExecutionUnit>)> {
        let mut analysis = analyze_statement(stmt)?;
        analysis.hint = hint_override;
        analysis.access_context = access_context_override;
        analysis.tenant = tenant_override;
        analysis.shadow_headers = shadow_headers_override
            .as_deref()
            .cloned()
            .unwrap_or_default();
        self.ensure_tenant_available(analysis.tenant.as_ref())?;
        self.resolve_lookup_sharding_conditions(raw, &mut analysis)
            .await?;
        self.reject_sharding_key_update(&analysis, stmt.values.as_ref())?;
        self.reject_ambiguous_lookup_mutation(&analysis)?;
        let mut plan = self.router.route(&analysis, force_primary)?;
        self.apply_tenant_route(&mut plan, analysis.tenant.as_ref());
        self.shadow_router.apply(&mut plan, &analysis);
        let statements = self.rewriter.rewrite(
            stmt,
            &analysis,
            &plan,
            self.plugin_registry.as_deref(),
        )?;
        if statements.len() != plan.targets.len() {
            return Err(ShardingError::Rewrite(format!(
                "rewritten statement count {} does not match route target count {}",
                statements.len(),
                plan.targets.len()
            )));
        }
        let units = plan
            .targets
            .iter()
            .cloned()
            .zip(statements)
            .map(|(target, statement)| ExecutionUnit {
                datasource: target.datasource,
                statement,
            })
            .collect();
        Ok((analysis, plan, units))
    }

    fn reject_sharding_key_update(
        &self,
        analysis: &StatementContext,
        values: Option<&Values>,
    ) -> Result<()> {
        if analysis.operation != SqlOperation::Update {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table() else {
            return Ok(());
        };
        let Some(rule) = self.config.table_rule(primary_table.full_name().as_str()) else {
            return Ok(());
        };
        let _ = values;
        if update_assigns_column(&analysis.ast, rule.sharding_column.as_str()) {
            return Err(ShardingError::Unsupported(format!(
                "updating sharding column `{}` on `{}` is not supported; use explicit reshard / move flow",
                rule.sharding_column, rule.logic_table
            )));
        }
        Ok(())
    }

    fn reject_ambiguous_lookup_mutation(&self, analysis: &StatementContext) -> Result<()> {
        if !matches!(
            analysis.operation,
            SqlOperation::Update | SqlOperation::Delete
        ) {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table() else {
            return Ok(());
        };
        for index in self
            .config
            .lookup_indexes_for(primary_table.full_name().as_str())
        {
            if analysis
                .exact_condition_value(index.lookup_column.as_str())
                .is_none()
            {
                return Err(ShardingError::Unsupported(format!(
                    "{} on `{}` with lookup index `{}` requires an exact `{}` predicate to keep lookup table consistent",
                    match analysis.operation {
                        SqlOperation::Update => "update",
                        SqlOperation::Delete => "delete",
                        _ => unreachable!(),
                    },
                    primary_table.full_name(),
                    index.lookup_table,
                    index.lookup_column
                )));
            }
        }
        Ok(())
    }

    fn apply_tenant_route(
        &self,
        plan: &mut RoutePlan,
        tenant: Option<&crate::tenant::TenantContext>,
    ) {
        for target in &mut plan.targets {
            for rewrite in &mut target.table_rewrites {
                if let Some(adjustment) = self.tenant_router.route(
                    target.datasource.clone(),
                    rewrite.actual_table.clone(),
                    tenant,
                ) {
                    target.datasource = adjustment.datasource;
                    rewrite.actual_table = adjustment.actual_table;
                }
            }
        }
    }

    pub(crate) async fn execute_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
        hint_override: Option<ShardingHint>,
        access_context_override: Option<ShardingAccessContext>,
        tenant_override: Option<crate::tenant::TenantContext>,
        shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
    ) -> std::result::Result<ExecResult, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(
                raw,
                &stmt,
                force_primary,
                hint_override,
                access_context_override,
                tenant_override,
                shadow_headers_override,
            )
            .await?;
        let result = self.executor.execute(raw, units).await.map_err(DbErr::from);
        if result.is_ok() {
            self.sync_lookup_table(raw, &analysis, stmt.values.as_ref())
                .await?;
        }
        self.audit(stmt.sql, &analysis, &plan, started_at.elapsed().as_millis());
        result
    }

    pub(crate) async fn query_one_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
        hint_override: Option<ShardingHint>,
        access_context_override: Option<ShardingAccessContext>,
        tenant_override: Option<crate::tenant::TenantContext>,
        shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(
                raw,
                &stmt,
                force_primary,
                hint_override,
                access_context_override,
                tenant_override,
                shadow_headers_override,
            )
            .await?;
        let result = self
            .executor
            .query_one(raw, units, &analysis, &plan, self.merger.as_ref())
            .await
            .map_err(DbErr::from);
        self.audit(stmt.sql, &analysis, &plan, started_at.elapsed().as_millis());
        result
    }

    pub(crate) async fn query_all_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
        hint_override: Option<ShardingHint>,
        access_context_override: Option<ShardingAccessContext>,
        tenant_override: Option<crate::tenant::TenantContext>,
        shadow_headers_override: Option<Arc<BTreeMap<String, String>>>,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(
                raw,
                &stmt,
                force_primary,
                hint_override,
                access_context_override,
                tenant_override,
                shadow_headers_override,
            )
            .await?;
        let result = self
            .executor
            .query_all(raw, units, &analysis, &plan, self.merger.as_ref())
            .await
            .map_err(DbErr::from);
        self.audit(stmt.sql, &analysis, &plan, started_at.elapsed().as_millis());
        result
    }

    async fn resolve_lookup_sharding_conditions(
        &self,
        raw: &dyn RawStatementExecutor,
        analysis: &mut StatementContext,
    ) -> Result<()> {
        if matches!(
            analysis.operation,
            SqlOperation::Insert | SqlOperation::Other
        ) {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table().cloned() else {
            return Ok(());
        };
        for index in self
            .config
            .lookup_indexes_for(primary_table.full_name().as_str())
        {
            if analysis
                .sharding_condition(index.sharding_column.as_str())
                .is_some()
            {
                continue;
            }
            let Some(lookup_value) = analysis
                .exact_condition_value(index.lookup_column.as_str())
                .cloned()
            else {
                continue;
            };
            let definition = LookupDefinition::from_config(index);
            self.lookup_index.register(definition.clone());
            let resolved = self
                .lookup_index
                .resolve(
                    primary_table.full_name().as_str(),
                    index.lookup_column.as_str(),
                    &lookup_value,
                )
                .or(self
                    .query_lookup_sharding_value(
                        raw,
                        &definition,
                        primary_table.schema.as_deref(),
                        &lookup_value,
                    )
                    .await?);
            if let Some(sharding_value) = resolved {
                analysis.sharding_conditions.insert(
                    normalize_column(index.sharding_column.as_str()),
                    crate::algorithm::ShardingCondition::Exact(sharding_value.clone()),
                );
                self.lookup_index.insert(
                    primary_table.full_name().as_str(),
                    index.lookup_column.as_str(),
                    &lookup_value,
                    sharding_value,
                );
            }
        }
        Ok(())
    }

    async fn query_lookup_sharding_value(
        &self,
        raw: &dyn RawStatementExecutor,
        definition: &LookupDefinition,
        fallback_schema: Option<&str>,
        lookup_value: &crate::algorithm::ShardingValue,
    ) -> Result<Option<crate::algorithm::ShardingValue>> {
        let datasource =
            self.lookup_datasource(definition.lookup_table.as_str(), fallback_schema)?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_select_sql(),
            [sharding_value_to_sea_value(lookup_value)],
        );
        let row = raw.query_one_for(datasource.as_str(), statement).await?;
        Ok(row.and_then(|row| {
            query_result_to_sharding_value(&row, definition.sharding_column.as_str())
        }))
    }

    async fn sync_lookup_table(
        &self,
        raw: &dyn RawStatementExecutor,
        analysis: &StatementContext,
        values: Option<&Values>,
    ) -> Result<()> {
        if !matches!(
            analysis.operation,
            SqlOperation::Insert | SqlOperation::Update | SqlOperation::Delete
        ) {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table().cloned() else {
            return Ok(());
        };
        for index in self
            .config
            .lookup_indexes_for(primary_table.full_name().as_str())
        {
            let definition = LookupDefinition::from_config(index);
            self.lookup_index.register(definition.clone());
            match analysis.operation {
                SqlOperation::Insert => {
                    let lookup_values = analysis.insert_values(index.lookup_column.as_str());
                    let sharding_values = analysis.insert_values(index.sharding_column.as_str());
                    for (lookup_value, sharding_value) in
                        lookup_values.iter().zip(sharding_values.iter())
                    {
                        self.upsert_lookup_entry(
                            raw,
                            &primary_table,
                            &definition,
                            lookup_value,
                            sharding_value,
                        )
                        .await?;
                    }
                }
                SqlOperation::Update => {
                    let old_lookup_value = analysis
                        .exact_condition_value(index.lookup_column.as_str())
                        .cloned();
                    let next_lookup_value = update_assignment_value(
                        &analysis.ast,
                        values,
                        index.lookup_column.as_str(),
                    );
                    let mut sharding_value = update_assignment_value(
                        &analysis.ast,
                        values,
                        index.sharding_column.as_str(),
                    )
                    .or_else(|| {
                        analysis
                            .exact_condition_value(index.sharding_column.as_str())
                            .cloned()
                    });
                    if sharding_value.is_none() {
                        if let Some(old_lookup) = old_lookup_value.as_ref() {
                            sharding_value = self
                                .query_lookup_sharding_value(
                                    raw,
                                    &definition,
                                    primary_table.schema.as_deref(),
                                    old_lookup,
                                )
                                .await?;
                        }
                    }

                    if let (Some(old_lookup), Some(next_lookup)) =
                        (old_lookup_value.as_ref(), next_lookup_value.as_ref())
                    {
                        if old_lookup != next_lookup {
                            self.delete_lookup_entry(raw, &primary_table, &definition, old_lookup)
                                .await?;
                        }
                    }

                    if let (Some(lookup_value), Some(sharding_value)) = (
                        next_lookup_value.or(old_lookup_value),
                        sharding_value.as_ref(),
                    ) {
                        self.upsert_lookup_entry(
                            raw,
                            &primary_table,
                            &definition,
                            &lookup_value,
                            sharding_value,
                        )
                        .await?;
                    }
                }
                SqlOperation::Delete => {
                    let Some(lookup_value) = analysis
                        .exact_condition_value(index.lookup_column.as_str())
                        .cloned()
                    else {
                        continue;
                    };
                    self.delete_lookup_entry(raw, &primary_table, &definition, &lookup_value)
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn upsert_lookup_entry(
        &self,
        raw: &dyn RawStatementExecutor,
        primary_table: &crate::router::QualifiedTableName,
        definition: &LookupDefinition,
        lookup_value: &crate::algorithm::ShardingValue,
        sharding_value: &crate::algorithm::ShardingValue,
    ) -> Result<()> {
        let datasource = self.lookup_datasource(
            definition.lookup_table.as_str(),
            primary_table.schema.as_deref(),
        )?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_upsert_sql(),
            [
                sharding_value_to_sea_value(lookup_value),
                sharding_value_to_sea_value(sharding_value),
            ],
        );
        raw.execute_for(datasource.as_str(), statement).await?;
        self.lookup_index.insert(
            primary_table.full_name().as_str(),
            definition.lookup_column.as_str(),
            lookup_value,
            sharding_value.clone(),
        );
        Ok(())
    }

    async fn delete_lookup_entry(
        &self,
        raw: &dyn RawStatementExecutor,
        primary_table: &crate::router::QualifiedTableName,
        definition: &LookupDefinition,
        lookup_value: &crate::algorithm::ShardingValue,
    ) -> Result<()> {
        let datasource = self.lookup_datasource(
            definition.lookup_table.as_str(),
            primary_table.schema.as_deref(),
        )?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_delete_sql(),
            [sharding_value_to_sea_value(lookup_value)],
        );
        raw.execute_for(datasource.as_str(), statement).await?;
        self.lookup_index.remove(
            primary_table.full_name().as_str(),
            definition.lookup_column.as_str(),
            lookup_value,
        );
        Ok(())
    }

    fn lookup_datasource(
        &self,
        lookup_table: &str,
        fallback_schema: Option<&str>,
    ) -> Result<String> {
        let (schema, _) = split_qualified_name(lookup_table);
        let schema = schema.or_else(|| fallback_schema.map(str::to_string));
        schema
            .as_deref()
            .and_then(|schema| self.config.schema_primary_datasource(schema))
            .or_else(|| self.config.default_datasource_name())
            .ok_or_else(|| ShardingError::Route("default datasource is not configured".to_string()))
            .map(str::to_string)
    }

    fn audit(&self, sql: String, analysis: &StatementContext, plan: &RoutePlan, duration_ms: u128) {
        if !self.config.audit.enabled {
            return;
        }
        let target_datasources = plan
            .targets
            .iter()
            .map(|target| target.datasource.clone())
            .collect::<Vec<_>>();
        record_fanout(FanoutMetric {
            rule_name: None,
            operation: analysis.operation,
            fanout: target_datasources.len(),
            targets: target_datasources.clone(),
        });
        if duration_ms >= self.config.audit.slow_query_threshold_ms as u128 {
            for datasource in target_datasources {
                record_slow_query(SlowQueryMetric {
                    datasource,
                    elapsed_ms: duration_ms,
                    threshold_ms: self.config.audit.slow_query_threshold_ms as u128,
                    reason: "query_execution".to_string(),
                });
            }
        }
        self.auditor.record(AuditEvent {
            sql,
            duration_ms,
            route: plan.clone(),
            is_slow_query: duration_ms >= self.config.audit.slow_query_threshold_ms as u128,
            full_scatter: self.config.audit.log_full_scatter && plan.targets.len() > 1,
            missing_sharding_key: self.config.audit.log_no_sharding_key
                && !analysis.has_sharding_key(),
        });
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
        tenant::{TenantContext, test_support},
    };

    use super::ShardingConnection;

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

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");

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

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");

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

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");

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

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&database_url)
            .await
            .expect("connect metadata database");

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

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");

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
        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");

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

        let normal = ShardingConnection::build(config.clone())
            .await
            .expect("build normal connection");
        let shadow = ShardingConnection::build(config)
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
