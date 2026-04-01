use crate::{
    connector::ShardingHint,
    error::Result,
    router::{QualifiedTableName, RouteTarget},
};

#[derive(Debug, Clone, Default)]
pub struct HintRouter;

impl HintRouter {
    pub fn route(
        &self,
        hint: &ShardingHint,
        datasource: &str,
        logic_table: Option<&QualifiedTableName>,
        all_targets: &[QualifiedTableName],
    ) -> Result<Option<Vec<RouteTarget>>> {
        match hint {
            ShardingHint::Table(table) => {
                let actual = QualifiedTableName::parse(table);
                Ok(Some(vec![RouteTarget {
                    datasource: datasource.to_string(),
                    table_rewrites: logic_table
                        .cloned()
                        .map(|logic_table| {
                            vec![crate::router::TableRewrite {
                                logic_table,
                                actual_table: actual,
                            }]
                        })
                        .unwrap_or_default(),
                }]))
            }
            ShardingHint::Broadcast => Ok(Some(
                all_targets
                    .iter()
                    .map(|actual_table| RouteTarget {
                        datasource: datasource.to_string(),
                        table_rewrites: logic_table
                            .cloned()
                            .map(|logic_table| {
                                vec![crate::router::TableRewrite {
                                    logic_table,
                                    actual_table: actual_table.clone(),
                                }]
                            })
                            .unwrap_or_default(),
                    })
                    .collect(),
            )),
            ShardingHint::Value(_, _) | ShardingHint::Shadow | ShardingHint::SkipMasking => {
                Ok(None)
            }
        }
    }

    pub fn override_sharding_value<'a>(
        &self,
        hint: &'a ShardingHint,
        sharding_column: &str,
    ) -> Option<&'a crate::algorithm::ShardingValue> {
        match hint {
            ShardingHint::Value(column, value) if column.eq_ignore_ascii_case(sharding_column) => {
                Some(value)
            }
            _ => None,
        }
    }

    pub fn requires_broadcast(&self, hint: &ShardingHint) -> bool {
        matches!(hint, ShardingHint::Broadcast)
    }
}
