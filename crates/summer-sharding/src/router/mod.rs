mod hint_router;
mod schema_router;
mod table_router;

use std::sync::Arc;

use crate::{
    algorithm::{AlgorithmRegistry, ShardingCondition, now_fixed_offset},
    config::ShardingConfig,
    connector::statement::StatementContext,
    error::{Result, ShardingError},
};

pub use crate::sql_rewrite::{QualifiedTableName, SqlOperation};
pub use hint_router::HintRouter;
pub use schema_router::SchemaRouter;
pub use table_router::TableRouter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderByItem {
    pub column: String,
    pub asc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTarget {
    pub datasource: String,
    pub table_rewrites: Vec<TableRewrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRewrite {
    pub logic_table: QualifiedTableName,
    pub actual_table: QualifiedTableName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    pub operation: SqlOperation,
    pub logic_tables: Vec<QualifiedTableName>,
    pub targets: Vec<RouteTarget>,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub broadcast: bool,
}

pub trait SqlRouter: Send + Sync + 'static {
    fn route(&self, analysis: &StatementContext, force_primary: bool) -> Result<RoutePlan>;
}

#[derive(Debug)]
pub struct DefaultSqlRouter {
    config: ShardingConfig,
    algorithms: AlgorithmRegistry,
    hint_router: HintRouter,
    schema_router: SchemaRouter,
    table_router: TableRouter,
}

impl DefaultSqlRouter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        let config = config.as_ref().clone();
        Self {
            hint_router: HintRouter,
            schema_router: SchemaRouter::new(&config),
            table_router: TableRouter::new(&config),
            algorithms: AlgorithmRegistry,
            config,
        }
    }
}

impl SqlRouter for DefaultSqlRouter {
    fn route(&self, analysis: &StatementContext, _force_primary: bool) -> Result<RoutePlan> {
        let Some(primary_table) = analysis.primary_table().cloned() else {
            let datasource = self
                .config
                .default_datasource_name()
                .ok_or_else(|| {
                    ShardingError::Route("default datasource is not configured".to_string())
                })?
                .to_string();
            return Ok(RoutePlan {
                operation: analysis.operation,
                logic_tables: Vec::new(),
                targets: vec![RouteTarget {
                    datasource,
                    table_rewrites: Vec::new(),
                }],
                order_by: analysis.order_by.clone(),
                limit: analysis.limit,
                offset: analysis.offset,
                broadcast: false,
            });
        };

        let logic_table = self
            .config
            .table_rule(primary_table.full_name().as_str())
            .map(|rule| QualifiedTableName::parse(rule.logic_table.as_str()))
            .unwrap_or_else(|| primary_table.clone());

        let datasource = self.schema_router.route(logic_table.schema.as_deref())?;

        if let Some(rule) = self.config.table_rule(logic_table.full_name().as_str()) {
            let algorithm = self.algorithms.build(rule)?;
            let available_targets = self.table_router.available_targets(rule, analysis)?;
            if let Some(hint) = &analysis.hint
                && let Some(targets) = self.hint_router.route(
                    hint,
                    datasource.as_str(),
                    Some(&logic_table),
                    &available_targets
                        .iter()
                        .map(|value| QualifiedTableName::parse(value))
                        .collect::<Vec<_>>(),
                )?
            {
                return Ok(RoutePlan {
                    operation: analysis.operation,
                    logic_tables: analysis.tables.clone(),
                    targets: self.apply_binding_group(targets, &logic_table, analysis)?,
                    order_by: analysis.order_by.clone(),
                    limit: analysis.limit,
                    offset: analysis.offset,
                    broadcast: self.hint_router.requires_broadcast(hint),
                });
            }
            let actual_targets = match analysis.operation {
                SqlOperation::Insert => {
                    let values = analysis.insert_values(rule.sharding_column.as_str());
                    if values.is_empty() {
                        return Err(ShardingError::MissingShardingValue {
                            table: rule.logic_table.clone(),
                            column: rule.sharding_column.clone(),
                        });
                    }
                    let mut targets = Vec::new();
                    for value in values {
                        targets.extend(algorithm.do_sharding(&available_targets, value));
                    }
                    targets
                }
                _ => {
                    if let Some(value) = analysis.hint.as_ref().and_then(|hint| {
                        self.hint_router
                            .override_sharding_value(hint, rule.sharding_column.as_str())
                    }) {
                        algorithm.do_sharding(&available_targets, value)
                    } else {
                        match analysis.sharding_condition(rule.sharding_column.as_str()) {
                            Some(ShardingCondition::Exact(value)) => {
                                algorithm.do_sharding(&available_targets, value)
                            }
                            Some(ShardingCondition::Range { lower, upper }) => {
                                match (lower.as_ref(), upper.as_ref()) {
                                    (Some(lower), Some(upper)) => algorithm.do_range_sharding(
                                        &available_targets,
                                        &lower.value,
                                        &upper.value,
                                    ),
                                    _ => self
                                        .table_router
                                        .expand_all_targets(rule, now_fixed_offset())?,
                                }
                            }
                            None => self
                                .table_router
                                .expand_all_targets(rule, now_fixed_offset())?,
                        }
                    }
                }
            };

            let mut actual_targets = actual_targets
                .into_iter()
                .map(|value| QualifiedTableName::parse(value.as_str()))
                .collect::<Vec<_>>();
            actual_targets.sort();
            actual_targets.dedup();

            return Ok(RoutePlan {
                operation: analysis.operation,
                logic_tables: analysis.tables.clone(),
                targets: self.apply_binding_group(
                    actual_targets
                        .into_iter()
                        .map(|actual_table| RouteTarget {
                            datasource: datasource.clone(),
                            table_rewrites: vec![TableRewrite {
                                logic_table: logic_table.clone(),
                                actual_table,
                            }],
                        })
                        .collect(),
                    &logic_table,
                    analysis,
                )?,
                order_by: analysis.order_by.clone(),
                limit: analysis.limit,
                offset: analysis.offset,
                broadcast: true,
            });
        }

        Ok(RoutePlan {
            operation: analysis.operation,
            logic_tables: analysis.tables.clone(),
            targets: vec![RouteTarget {
                datasource,
                table_rewrites: vec![TableRewrite {
                    logic_table: logic_table.clone(),
                    actual_table: logic_table,
                }],
            }],
            order_by: analysis.order_by.clone(),
            limit: analysis.limit,
            offset: analysis.offset,
            broadcast: self
                .config
                .is_broadcast_table(primary_table.full_name().as_str()),
        })
    }
}

impl DefaultSqlRouter {
    fn apply_binding_group(
        &self,
        targets: Vec<RouteTarget>,
        primary_logic_table: &QualifiedTableName,
        analysis: &StatementContext,
    ) -> Result<Vec<RouteTarget>> {
        let Some(group) = self
            .config
            .binding_group_for(primary_logic_table.full_name().as_str())
        else {
            return Ok(targets);
        };

        let primary_actual_targets =
            self.binding_group_targets_for(primary_logic_table, analysis)?;

        targets
            .into_iter()
            .map(|mut target| {
                let primary_target_index = target.table_rewrites.first().and_then(|rewrite| {
                    primary_actual_targets
                        .iter()
                        .position(|candidate| candidate == &rewrite.actual_table)
                });

                for logic_table in &analysis.tables {
                    if logic_table == primary_logic_table {
                        continue;
                    }
                    if !group.tables.iter().any(|table| {
                        table.eq_ignore_ascii_case(logic_table.full_name().as_str())
                            || table
                                .split_once('.')
                                .map(|(_, table)| table)
                                .unwrap_or(table)
                                .eq_ignore_ascii_case(logic_table.table.as_str())
                    }) {
                        continue;
                    }
                    let actual_table = self
                        .binding_group_targets_for(logic_table, analysis)?
                        .into_iter()
                        .nth(primary_target_index.unwrap_or(0))
                        .unwrap_or_else(|| logic_table.clone());
                    target.table_rewrites.push(TableRewrite {
                        logic_table: logic_table.clone(),
                        actual_table,
                    });
                }
                Ok(target)
            })
            .collect()
    }

    fn binding_group_targets_for(
        &self,
        logic_table: &QualifiedTableName,
        analysis: &StatementContext,
    ) -> Result<Vec<QualifiedTableName>> {
        let Some(rule) = self.config.table_rule(logic_table.full_name().as_str()) else {
            return Ok(vec![logic_table.clone()]);
        };

        let available_targets = self.table_router.available_targets(rule, analysis)?;
        let algorithm = self.algorithms.build(rule)?;
        let actual_targets = match analysis.operation {
            SqlOperation::Insert => {
                let values = analysis.insert_values(rule.sharding_column.as_str());
                if values.is_empty() {
                    return Err(ShardingError::MissingShardingValue {
                        table: rule.logic_table.clone(),
                        column: rule.sharding_column.clone(),
                    });
                }
                let mut targets = Vec::new();
                for value in values {
                    targets.extend(algorithm.do_sharding(&available_targets, value));
                }
                targets
            }
            _ => {
                if let Some(value) = analysis.hint.as_ref().and_then(|hint| {
                    self.hint_router
                        .override_sharding_value(hint, rule.sharding_column.as_str())
                }) {
                    algorithm.do_sharding(&available_targets, value)
                } else {
                    match analysis.sharding_condition(rule.sharding_column.as_str()) {
                        Some(ShardingCondition::Exact(value)) => {
                            algorithm.do_sharding(&available_targets, value)
                        }
                        Some(ShardingCondition::Range { lower, upper }) => {
                            match (lower.as_ref(), upper.as_ref()) {
                                (Some(lower), Some(upper)) => algorithm.do_range_sharding(
                                    &available_targets,
                                    &lower.value,
                                    &upper.value,
                                ),
                                _ => self
                                    .table_router
                                    .expand_all_targets(rule, now_fixed_offset())?,
                            }
                        }
                        None => self
                            .table_router
                            .expand_all_targets(rule, now_fixed_offset())?,
                    }
                }
            }
        };

        let mut actual_targets = actual_targets
            .into_iter()
            .map(|value| QualifiedTableName::parse(value.as_str()))
            .collect::<Vec<_>>();
        actual_targets.sort();
        actual_targets.dedup();
        Ok(actual_targets)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use sea_orm::{DbBackend, Statement};

    use crate::connector::analyze_statement;

    use super::{DefaultSqlRouter, QualifiedTableName, SqlOperation, SqlRouter};

    #[test]
    fn router_exports_shared_sql_rewrite_types() {
        let table: crate::sql_rewrite::QualifiedTableName = QualifiedTableName::parse("sys.user");
        let operation: crate::sql_rewrite::SqlOperation = SqlOperation::Select;

        assert_eq!(table.full_name(), "sys.user");
        assert_eq!(operation, crate::sql_rewrite::SqlOperation::Select);
    }

    #[test]
    fn router_rewrites_binding_tables_with_independent_actual_table_names() {
        let config = Arc::new(
            crate::config::ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [[sharding.tables]]
                logic_table = "ai.request"
                actual_tables = ["ai.req_even", "ai.req_odd"]
                sharding_column = "tenant_id"
                algorithm = "hash_mod"

                  [sharding.tables.algorithm_props]
                  count = 2

                [[sharding.tables]]
                logic_table = "ai.request_execution"
                actual_tables = ["ai.exec_bucket_even", "ai.exec_bucket_odd"]
                sharding_column = "tenant_id"
                algorithm = "hash_mod"

                  [sharding.tables.algorithm_props]
                  count = 2

                [[sharding.binding_groups]]
                tables = ["ai.request", "ai.request_execution"]
                sharding_column = "tenant_id"
                "#,
            )
            .expect("config"),
        );
        let router = DefaultSqlRouter::new(config);
        let analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            r#"SELECT r.id, e.status
               FROM ai.request r
               JOIN ai.request_execution e ON r.id = e.request_id
               WHERE r.tenant_id = 1"#,
        ))
        .expect("analysis");

        let plan = router.route(&analysis, false).expect("route");

        assert_eq!(plan.targets.len(), 1);
        let rewrites = &plan.targets[0].table_rewrites;
        let actuals = rewrites
            .iter()
            .map(|rewrite| {
                (
                    rewrite.logic_table.full_name(),
                    rewrite.actual_table.full_name(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            actuals.get("ai.request").map(String::as_str),
            Some("ai.req_odd")
        );
        assert_eq!(
            actuals.get("ai.request_execution").map(String::as_str),
            Some("ai.exec_bucket_odd")
        );
    }
}
