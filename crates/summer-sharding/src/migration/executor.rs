use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::{
    cdc::{CdcCutover, CdcPipeline, CdcSink, CdcSinkKind, CdcSource, CdcTask, RowTransform},
    error::{Result, ShardingError},
    migration::{MigrationExecutionPlan, MigrationPhase, MigrationTaskKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationExecutionOptions {
    pub slot: Option<String>,
    pub publication: Option<String>,
    pub start_position: Option<String>,
    pub max_catch_up_polls: usize,
}

impl Default for MigrationExecutionOptions {
    fn default() -> Self {
        Self {
            slot: None,
            publication: None,
            start_position: None,
            max_catch_up_polls: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationExecutionReport {
    pub name: String,
    pub kind: MigrationTaskKind,
    pub phases: Vec<MigrationPhase>,
    pub snapshot_written: usize,
    pub catch_up_written: usize,
    pub last_position: Option<String>,
    pub cutover_complete: bool,
    pub cleanup_statements: usize,
}

#[async_trait]
pub trait MigrationCleanup: Send + Sync + 'static {
    async fn cleanup(&self, plan: &MigrationExecutionPlan) -> Result<usize>;
}

#[derive(Debug, Clone, Default)]
pub struct NoopMigrationCleanup;

#[async_trait]
impl MigrationCleanup for NoopMigrationCleanup {
    async fn cleanup(&self, _plan: &MigrationExecutionPlan) -> Result<usize> {
        Ok(0)
    }
}

#[derive(Debug, Clone)]
pub struct SqlMigrationCleanup {
    connection: Arc<DatabaseConnection>,
}

impl SqlMigrationCleanup {
    pub fn new(connection: Arc<DatabaseConnection>) -> Self {
        Self { connection }
    }

    pub fn statements(plan: &MigrationExecutionPlan) -> Vec<String> {
        if !plan.cleanup_delete_source {
            return Vec::new();
        }

        let filter_clause = plan
            .source_filter
            .as_ref()
            .map(|filter| format!(" WHERE {filter}"))
            .unwrap_or_default();

        plan.source_tables
            .iter()
            .map(|table| format!("DELETE FROM {table}{filter_clause}"))
            .collect()
    }
}

#[async_trait]
impl MigrationCleanup for SqlMigrationCleanup {
    async fn cleanup(&self, plan: &MigrationExecutionPlan) -> Result<usize> {
        let statements = Self::statements(plan);
        for statement in &statements {
            self.connection.execute_unprepared(statement).await?;
        }
        Ok(statements.len())
    }
}

#[derive(Debug, Clone, Default)]
pub struct MigrationExecutor {
    pipeline: CdcPipeline,
}

impl MigrationExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_cdc_task(
        &self,
        plan: &MigrationExecutionPlan,
        options: &MigrationExecutionOptions,
    ) -> CdcTask {
        CdcTask {
            name: plan.name.clone(),
            source_tables: plan.source_tables.clone(),
            source_filter: plan.source_filter.clone(),
            batch_size: plan.batch_size as i64,
            slot: Some(
                options
                    .slot
                    .clone()
                    .unwrap_or_else(|| format!("{}_slot", sanitize_identifier(plan.name.as_str()))),
            ),
            publication: Some(
                options
                    .publication
                    .clone()
                    .unwrap_or_else(|| format!("{}_pub", sanitize_identifier(plan.name.as_str()))),
            ),
            start_position: options.start_position.clone(),
            max_catch_up_polls: options.max_catch_up_polls.max(1),
        }
    }

    pub async fn execute(
        &self,
        plan: &MigrationExecutionPlan,
        source: &dyn CdcSource,
        transformer: &dyn RowTransform,
        sink: &dyn CdcSink,
        cutover: Option<&dyn CdcCutover>,
        cleanup: Option<&dyn MigrationCleanup>,
    ) -> Result<MigrationExecutionReport> {
        self.execute_with_options(
            plan,
            source,
            transformer,
            sink,
            cutover,
            cleanup,
            &MigrationExecutionOptions::default(),
        )
        .await
    }

    pub async fn execute_with_options(
        &self,
        plan: &MigrationExecutionPlan,
        source: &dyn CdcSource,
        transformer: &dyn RowTransform,
        sink: &dyn CdcSink,
        cutover: Option<&dyn CdcCutover>,
        cleanup: Option<&dyn MigrationCleanup>,
        options: &MigrationExecutionOptions,
    ) -> Result<MigrationExecutionReport> {
        validate_plan_runtime(plan, transformer, sink)?;
        let task = self.build_cdc_task(plan, options);
        let report = self
            .pipeline
            .run(&task, source, transformer, sink, cutover)
            .await?;

        let mut phases = report
            .phases
            .into_iter()
            .map(|phase| match phase {
                crate::cdc::CdcPhase::Snapshot => MigrationPhase::Snapshot,
                crate::cdc::CdcPhase::CatchUp => MigrationPhase::CatchUp,
                crate::cdc::CdcPhase::CutOver => MigrationPhase::CutOver,
            })
            .collect::<Vec<_>>();

        let cleanup_statements = if plan.cleanup_delete_source {
            let cleanup = cleanup.ok_or_else(|| {
                ShardingError::Unsupported(format!(
                    "migration plan `{}` requires cleanup handler",
                    plan.name
                ))
            })?;
            phases.push(MigrationPhase::Cleanup);
            cleanup.cleanup(plan).await?
        } else {
            0
        };

        Ok(MigrationExecutionReport {
            name: plan.name.clone(),
            kind: plan.kind.clone(),
            phases,
            snapshot_written: report.snapshot_written,
            catch_up_written: report.catch_up_written,
            last_position: report.last_position,
            cutover_complete: report.cutover_complete,
            cleanup_statements,
        })
    }
}

fn sanitize_identifier(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if sanitized.is_empty() {
        sanitized.push('_');
    }
    if sanitized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        sanitized.insert(0, '_');
    }
    sanitized
}

fn validate_plan_runtime(
    plan: &MigrationExecutionPlan,
    transformer: &dyn RowTransform,
    sink: &dyn CdcSink,
) -> Result<()> {
    validate_sink(plan, sink)?;
    validate_transformer(plan, transformer, sink)?;
    Ok(())
}

fn validate_sink(plan: &MigrationExecutionPlan, sink: &dyn CdcSink) -> Result<()> {
    let sink_kind = sink.kind();
    match &plan.sink {
        crate::migration::MigrationSink::ClickHouse { .. } => {
            if sink_kind != CdcSinkKind::ClickHouse {
                return Err(ShardingError::Unsupported(format!(
                    "migration plan `{}` expects a ClickHouse sink, got `{}`",
                    plan.name,
                    sink.descriptor()
                )));
            }
        }
        crate::migration::MigrationSink::Schema { .. } => {
            if sink_kind != CdcSinkKind::DirectTable {
                return Err(ShardingError::Unsupported(format!(
                    "migration plan `{}` expects a direct relational sink for schema migration, got `{}`",
                    plan.name,
                    sink.descriptor()
                )));
            }
        }
        crate::migration::MigrationSink::Tables { .. } => {
            if sink_kind == CdcSinkKind::ClickHouse {
                return Err(ShardingError::Unsupported(format!(
                    "migration plan `{}` expects relational sink semantics, got `{}`",
                    plan.name,
                    sink.descriptor()
                )));
            }
            if plan
                .transformer
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("rehash"))
                && sink_kind != CdcSinkKind::HashSharded
            {
                return Err(ShardingError::Unsupported(format!(
                    "migration plan `{}` expects a hash-sharded sink for transformer `rehash`, got `{}`",
                    plan.name,
                    sink.descriptor()
                )));
            }
        }
    }
    Ok(())
}

fn validate_transformer(
    plan: &MigrationExecutionPlan,
    transformer: &dyn RowTransform,
    sink: &dyn CdcSink,
) -> Result<()> {
    let Some(expected) = plan.transformer.as_deref() else {
        return Ok(());
    };
    let expected = normalize_descriptor(expected);
    let transformer_descriptor = normalize_descriptor(transformer.descriptor());
    let compatible = transformer_descriptor.contains(expected.as_str())
        || (expected == "rehash" && sink.kind() == CdcSinkKind::HashSharded);
    if compatible {
        Ok(())
    } else {
        Err(ShardingError::Unsupported(format!(
            "migration plan `{}` expects transformer `{}`, got `{}` with sink `{}`",
            plan.name,
            plan.transformer.as_deref().unwrap_or_default(),
            transformer.descriptor(),
            sink.descriptor()
        )))
    }
}

fn normalize_descriptor(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}
