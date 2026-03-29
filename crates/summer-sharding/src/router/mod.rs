mod hint_router;
mod rw_router;
mod schema_router;
mod table_router;

use std::sync::Arc;

use sqlparser::ast::{Ident, ObjectName};

use crate::{
    algorithm::{AlgorithmRegistry, ShardingCondition, now_fixed_offset},
    config::ShardingConfig,
    connector::statement::StatementContext,
    error::{Result, ShardingError},
    lookup::{LookupDefinition, LookupIndex},
};

pub use hint_router::HintRouter;
pub use rw_router::ReadWriteRouter;
pub use schema_router::SchemaRouter;
pub use table_router::TableRouter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlOperation {
    Select,
    Insert,
    Update,
    Delete,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedTableName {
    pub schema: Option<String>,
    pub table: String,
}

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
    config: Arc<ShardingConfig>,
    algorithms: AlgorithmRegistry,
    hint_router: HintRouter,
    schema_router: SchemaRouter,
    table_router: TableRouter,
    read_write_router: ReadWriteRouter,
    lookup_index: Arc<LookupIndex>,
}

impl DefaultSqlRouter {
    pub fn new(config: Arc<ShardingConfig>, lookup_index: Arc<LookupIndex>) -> Self {
        Self {
            hint_router: HintRouter,
            schema_router: SchemaRouter::new(config.clone()),
            table_router: TableRouter::new(config.clone()),
            read_write_router: ReadWriteRouter::new(config.clone()),
            algorithms: AlgorithmRegistry,
            lookup_index,
            config,
        }
    }
}

impl SqlRouter for DefaultSqlRouter {
    fn route(&self, analysis: &StatementContext, force_primary: bool) -> Result<RoutePlan> {
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
                    datasource: self.read_write_router.route(
                        datasource.as_str(),
                        analysis.operation,
                        force_primary,
                    ),
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

        let base_datasource = self.schema_router.route(logic_table.schema.as_deref())?;
        let default_datasource = self.read_write_router.route(
            base_datasource.as_str(),
            analysis.operation,
            force_primary,
        );

        if let Some(rule) = self.config.table_rule(logic_table.full_name().as_str()) {
            let algorithm = self.algorithms.build(rule)?;
            let available_targets = self.table_router.available_targets(rule, analysis)?;
            if let Some(hint) = &analysis.hint {
                if let Some(targets) = self.hint_router.route(
                    hint,
                    default_datasource.as_str(),
                    Some(&logic_table),
                    &available_targets
                        .iter()
                        .map(|value| QualifiedTableName::parse(value))
                        .collect::<Vec<_>>(),
                )? {
                    return Ok(RoutePlan {
                        operation: analysis.operation,
                        logic_tables: analysis.tables.clone(),
                        targets: self.apply_binding_group(targets, &logic_table, analysis),
                        order_by: analysis.order_by.clone(),
                        limit: analysis.limit,
                        offset: analysis.offset,
                        broadcast: self.hint_router.requires_broadcast(hint),
                    });
                }
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
                    } else if let Some(value) =
                        self.resolve_lookup_sharding_value(&logic_table, analysis, rule)
                    {
                        algorithm.do_sharding(&available_targets, &value)
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
                            datasource: self.read_write_router.route(
                                default_datasource.as_str(),
                                analysis.operation,
                                force_primary,
                            ),
                            table_rewrites: vec![TableRewrite {
                                logic_table: logic_table.clone(),
                                actual_table,
                            }],
                        })
                        .collect(),
                    &logic_table,
                    analysis,
                ),
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
                datasource: default_datasource,
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
    fn resolve_lookup_sharding_value(
        &self,
        logic_table: &QualifiedTableName,
        analysis: &StatementContext,
        rule: &crate::config::TableRuleConfig,
    ) -> Option<crate::algorithm::ShardingValue> {
        self.config
            .lookup_indexes_for(logic_table.full_name().as_str())
            .into_iter()
            .filter(|index| {
                index
                    .sharding_column
                    .eq_ignore_ascii_case(rule.sharding_column.as_str())
            })
            .find_map(|index| {
                self.lookup_index
                    .register(LookupDefinition::from_config(index));
                analysis
                    .exact_condition_value(index.lookup_column.as_str())
                    .and_then(|value| {
                        self.lookup_index.resolve(
                            logic_table.full_name().as_str(),
                            index.lookup_column.as_str(),
                            value,
                        )
                    })
            })
    }

    fn apply_binding_group(
        &self,
        targets: Vec<RouteTarget>,
        primary_logic_table: &QualifiedTableName,
        analysis: &StatementContext,
    ) -> Vec<RouteTarget> {
        let Some(group) = self
            .config
            .binding_group_for(primary_logic_table.full_name().as_str())
        else {
            return targets;
        };

        targets
            .into_iter()
            .map(|mut target| {
                let suffix = target
                    .table_rewrites
                    .first()
                    .and_then(|rewrite| {
                        rewrite
                            .actual_table
                            .table
                            .strip_prefix(primary_logic_table.table.as_str())
                    })
                    .unwrap_or("")
                    .to_string();

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
                    target.table_rewrites.push(TableRewrite {
                        logic_table: logic_table.clone(),
                        actual_table: QualifiedTableName {
                            schema: logic_table.schema.clone(),
                            table: format!("{}{}", logic_table.table, suffix),
                        },
                    });
                }
                target
            })
            .collect()
    }
}

impl QualifiedTableName {
    pub fn parse(value: &str) -> Self {
        match value.split_once('.') {
            Some((schema, table)) => Self {
                schema: Some(schema.to_string()),
                table: table.to_string(),
            },
            None => Self {
                schema: None,
                table: value.to_string(),
            },
        }
    }

    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{schema}.{}", self.table),
            None => self.table.clone(),
        }
    }

    pub fn to_object_name(&self) -> ObjectName {
        match &self.schema {
            Some(schema) => ObjectName(vec![Ident::new(schema), Ident::new(&self.table)]),
            None => ObjectName(vec![Ident::new(&self.table)]),
        }
    }

    pub fn matches_object_name(&self, name: &ObjectName) -> bool {
        match name.0.as_slice() {
            [table] => table.value.eq_ignore_ascii_case(self.table.as_str()),
            [schema, table] => {
                self.schema
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(schema.value.as_str()))
                    && table.value.eq_ignore_ascii_case(self.table.as_str())
            }
            _ => false,
        }
    }
}
