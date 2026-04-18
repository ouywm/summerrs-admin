use std::time::Instant;

use sea_orm::{DbErr, ExecResult, QueryResult, Statement, Values};

use super::{ExecutionOverrides, ShardingConnection, ShardingConnectionInner};
use crate::{
    connector::statement::{StatementContext, analyze_statement},
    error::{Result, ShardingError},
    execute::{ExecutionUnit, RawStatementExecutor},
    lookup::update_assigns_column,
    router::{RoutePlan, SqlOperation},
};

impl ShardingConnection {
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
                self.execution_overrides(),
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
            .execute_with_raw(raw, stmt, force_primary, self.execution_overrides())
            .await
    }

    pub(crate) async fn query_one_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        self.inner
            .query_one_with_raw(raw, stmt, force_primary, self.execution_overrides())
            .await
    }

    pub(crate) async fn query_all_with_raw(
        &self,
        raw: &dyn RawStatementExecutor,
        stmt: Statement,
        force_primary: bool,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        self.inner
            .query_all_with_raw(raw, stmt, force_primary, self.execution_overrides())
            .await
    }
}

impl ShardingConnectionInner {
    pub(super) fn ensure_tenant_available(
        &self,
        tenant: Option<&crate::tenant::TenantContext>,
    ) -> Result<()> {
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
        overrides: ExecutionOverrides,
    ) -> Result<(StatementContext, RoutePlan, Vec<ExecutionUnit>)> {
        let mut analysis = analyze_statement(stmt)?;
        analysis.hint = overrides.hint;
        analysis.access_context = overrides.access_context;
        analysis.tenant = overrides.tenant;
        analysis.shadow_headers = overrides
            .shadow_headers
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
        let statements =
            self.rewriter
                .rewrite(stmt, &analysis, &plan, self.plugin_registry.get())?;
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
        overrides: ExecutionOverrides,
    ) -> std::result::Result<ExecResult, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(raw, &stmt, force_primary, overrides)
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
        overrides: ExecutionOverrides,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(raw, &stmt, force_primary, overrides)
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
        overrides: ExecutionOverrides,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        let started_at = Instant::now();
        let (analysis, plan, units) = self
            .prepare_statement(raw, &stmt, force_primary, overrides)
            .await?;
        let result = self
            .executor
            .query_all(raw, units, &analysis, &plan, self.merger.as_ref())
            .await
            .map_err(DbErr::from);
        self.audit(stmt.sql, &analysis, &plan, started_at.elapsed().as_millis());
        result
    }
}
