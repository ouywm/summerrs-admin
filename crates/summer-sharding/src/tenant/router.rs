use std::sync::Arc;

use crate::{
    algorithm::normalize_tenant_suffix,
    config::{ShardingConfig, TenantIsolationLevel},
    router::QualifiedTableName,
    tenant::{TenantContext, TenantMetadataStore},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantRouteAdjustment {
    pub datasource: String,
    pub actual_table: QualifiedTableName,
    pub tenant_context: TenantContext,
}

#[derive(Debug, Clone)]
pub struct TenantRouter {
    config: ShardingConfig,
    metadata: Arc<TenantMetadataStore>,
}

impl TenantRouter {
    pub fn new(config: Arc<ShardingConfig>, metadata: Arc<TenantMetadataStore>) -> Self {
        Self {
            config: config.as_ref().clone(),
            metadata,
        }
    }

    pub fn resolve_context(&self, mut context: TenantContext) -> TenantContext {
        if let Some(metadata) = self.metadata.get(context.tenant_id.as_str()) {
            context.isolation_level = metadata.isolation_level;
            if context.schema_override.is_none() {
                context.schema_override = metadata.schema_name;
            }
            if context.datasource_override.is_none() {
                context.datasource_override = metadata.datasource_name;
            }
        } else if context.isolation_level == TenantIsolationLevel::SharedRow {
            context.isolation_level = self.config.default_tenant_isolation();
        }
        context
    }

    pub fn route(
        &self,
        datasource: String,
        actual_table: QualifiedTableName,
        tenant: Option<&TenantContext>,
    ) -> Option<TenantRouteAdjustment> {
        let context = tenant.cloned()?;
        if self
            .config
            .is_tenant_shared_table(actual_table.full_name().as_str())
        {
            return Some(TenantRouteAdjustment {
                datasource,
                actual_table,
                tenant_context: context,
            });
        }

        let routed_table = match context.isolation_level {
            TenantIsolationLevel::SharedRow => actual_table,
            TenantIsolationLevel::SeparateTable => QualifiedTableName {
                schema: actual_table.schema.clone(),
                table: format!(
                    "{}_{}",
                    actual_table.table,
                    normalize_tenant_suffix(context.tenant_id.as_str())
                ),
            },
            TenantIsolationLevel::SeparateSchema => QualifiedTableName {
                schema: context.schema_override.clone().or_else(|| {
                    Some(format!(
                        "tenant_{}",
                        normalize_tenant_suffix(context.tenant_id.as_str())
                    ))
                }),
                table: actual_table.table,
            },
            TenantIsolationLevel::SeparateDatabase => actual_table,
        };

        let routed_datasource = if context.isolation_level == TenantIsolationLevel::SeparateDatabase
        {
            context
                .datasource_override
                .clone()
                .unwrap_or(datasource.clone())
        } else {
            datasource
        };

        Some(TenantRouteAdjustment {
            datasource: routed_datasource,
            actual_table: routed_table,
            tenant_context: context,
        })
    }
}
