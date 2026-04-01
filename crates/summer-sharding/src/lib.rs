pub mod algorithm;
pub mod audit;
pub mod cdc;
pub mod config;
pub mod connector;
pub mod datasource;
pub mod ddl;
pub mod encrypt;
pub mod error;
pub mod execute;
pub mod extensions;
pub mod keygen;
pub mod lookup;
pub mod masking;
pub mod merge;
pub mod migration;
pub mod plugin;
pub mod rewrite;
pub mod rewrite_plugin;
pub mod router;
pub mod shadow;
pub mod tenant;
#[cfg(feature = "web")]
pub mod web;

pub use algorithm::{
    AlgorithmRegistry, ComplexShardingAlgorithm, ShardingAlgorithm, ShardingCondition,
    ShardingValue, TenantShardingAlgorithm, TimeGranularity,
};
pub use audit::{AuditEvent, DefaultSqlAuditor, SqlAuditor};
pub use cdc::{
    CdcBatch, CdcCutover, CdcOperation, CdcPhase, CdcPipeline, CdcRecord, CdcRunReport, CdcSink,
    CdcSinkKind, CdcSource, CdcSubscribeRequest, CdcSubscription, CdcTask, ClickHouseHttpSink,
    InMemoryCdcSource, PgCdcSource, PgSourcePosition, PostgresHashShardSink, PostgresTableSink,
    RowTransform, RowTransformer, SqlCdcCutover, SqlCdcSink, SqlCdcSinkBuilder, SqlCdcSource,
    SqlStatementTemplate, TableSink,
};
pub use config::{
    ActualTablesConfig, AuditConfig, BindingGroupConfig, CdcConfig, CdcTaskConfig, ConfigProps,
    DataSourceConfig, DataSourceRole, EncryptConfig, EncryptRuleConfig, KeyGeneratorConfig,
    LoadBalanceKind, LookupIndexConfig, MaskingConfig, MaskingRuleConfig, OnlineDdlConfig,
    ReadWriteRuleConfig, ReadWriteSplittingConfig, ShadowConditionConfig, ShadowConditionKind,
    ShadowConfig, ShadowDatabaseModeConfig, ShadowTableModeConfig, ShardingConfig,
    ShardingGlobalConfig, ShardingSectionConfig, SummerShardingConfig, TableRuleConfig,
    TenantConfig, TenantIdSource, TenantIsolationLevel, TenantRowLevelConfig,
    TenantRowLevelStrategy,
};
pub use connector::hint::{ShardingAccessContext, should_skip_masking, with_access_context};
pub use connector::{
    PreparedTwoPhaseTransaction, ShardingConnection, ShardingHint, ShardingTransaction,
    TwoPhaseShardingTransaction, TwoPhaseTransactionError, analyze_statement, with_hint,
};
pub use datasource::{
    DataSourceDiscovery, DataSourceHealth, DataSourcePool, DataSourceRouteState, FanoutMetric,
    InMemoryRuntimeRecorder, RuntimeMetricsSnapshot, RuntimeRecorder, ShardHitMetric,
    SlowQueryMetric, clear_route_states, record_fanout, record_shard_hit, record_slow_query,
    reset_runtime_recorder, route_state, runtime_recorder, set_route_state, set_runtime_recorder,
};
pub use ddl::{
    DdlProgress, DdlScheduler, DdlShardPlan, DdlTaskId, DdlTaskStatus, GhostTablePlan,
    GhostTablePlanner, InMemoryOnlineDdlEngine, OnlineDdlEngine, OnlineDdlTask,
};
pub use encrypt::{AesGcmEncryptor, DigestAlgorithm, EncryptAlgorithm};
pub use error::{Result, ShardingError};
pub use execute::{
    ExecutionUnit, Executor, RawStatementExecutor, ScatterGatherExecutor, SimpleExecutor,
};
pub use keygen::{KeyGenerator, KeyGeneratorRegistry, SnowflakeKeyGenerator, TsidKeyGenerator};
pub use lookup::LookupIndex;
pub use masking::{EmailMasking, IpMasking, MaskingAlgorithm, PartialMasking, PhoneMasking};
pub use merge::{DefaultResultMerger, ResultMerger};
pub use migration::{
    ArchiveCandidate, ArchivePlanner, AutoTablePlanner, MigrationCleanup,
    MigrationExecutionOptions, MigrationExecutionPlan, MigrationExecutionReport,
    MigrationExecutionStep, MigrationExecutor, MigrationOrchestrator, MigrationPhase,
    MigrationPlan, MigrationSink, MigrationTaskKind, NoopMigrationCleanup, ReshardingMove,
    ReshardingPlanner, SqlMigrationCleanup,
};
pub use plugin::SummerShardingPlugin;
pub use rewrite::{DefaultSqlRewriter, SqlRewriter};
pub use rewrite_plugin::{
    PluginRegistry, RewriteContext, ShardingRewriteConfigurator, ShardingRouteInfo,
    SqlRewritePlugin, TableRewritePair, helpers as rewrite_helpers,
};
pub use router::{
    DefaultSqlRouter, OrderByItem, QualifiedTableName, RoutePlan, RouteTarget, SqlOperation,
    SqlRouter, TableRewrite,
};
pub use tenant::{
    TenantContext, TenantLifecycleManager, TenantMetadataApplyOutcome, TenantMetadataEvent,
    TenantMetadataEventKind, TenantMetadataRecord, TenantMetadataStore, TenantRouter,
};
#[cfg(feature = "web")]
pub use web::{CurrentTenant, OptionalCurrentTenant, TenantContextLayer, TenantShardingConnection};
