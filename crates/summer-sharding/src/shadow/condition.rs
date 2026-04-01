use std::{collections::BTreeMap, sync::Arc};

use crate::{
    config::{ShadowConditionKind, ShardingConfig},
    connector::{ShardingHint, statement::StatementContext},
    router::{QualifiedTableName, RoutePlan},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShadowCondition {
    Header {
        key: String,
        value: Option<String>,
    },
    Column {
        column: String,
        value: Option<String>,
    },
    Hint,
}

#[derive(Debug, Clone)]
pub struct ShadowRouter {
    config: ShardingConfig,
}

impl ShadowRouter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self {
            config: config.as_ref().clone(),
        }
    }

    pub fn should_route(&self, analysis: &StatementContext) -> bool {
        self.should_route_with_headers(analysis, &analysis.shadow_headers)
    }

    pub fn should_route_with_headers(
        &self,
        analysis: &StatementContext,
        headers: &BTreeMap<String, String>,
    ) -> bool {
        if !self.config.shadow.enabled {
            return false;
        }
        if matches!(analysis.hint, Some(ShardingHint::Shadow)) {
            return true;
        }

        self.config
            .shadow
            .conditions
            .iter()
            .any(|condition| match condition.kind {
                ShadowConditionKind::Hint => matches!(analysis.hint, Some(ShardingHint::Shadow)),
                ShadowConditionKind::Header => condition
                    .key
                    .as_ref()
                    .and_then(|key| header_value(headers, key))
                    .is_some_and(|actual| {
                        condition
                            .value
                            .as_ref()
                            .is_none_or(|expected| expected.eq_ignore_ascii_case(actual))
                    }),
                ShadowConditionKind::Column => condition
                    .column
                    .as_ref()
                    .and_then(|column| analysis.exact_condition_value(column))
                    .and_then(|value| match value {
                        crate::algorithm::ShardingValue::Str(text) => Some(text.clone()),
                        crate::algorithm::ShardingValue::Int(number) => Some(number.to_string()),
                        crate::algorithm::ShardingValue::DateTime(datetime) => {
                            Some(datetime.to_rfc3339())
                        }
                        crate::algorithm::ShardingValue::Null => None,
                    })
                    .is_some_and(|actual| {
                        condition
                            .value
                            .as_ref()
                            .is_none_or(|expected| expected.eq_ignore_ascii_case(actual.as_str()))
                    }),
            })
    }

    pub fn apply(&self, plan: &mut RoutePlan, analysis: &StatementContext) {
        if !self.should_route_with_headers(analysis, &analysis.shadow_headers) {
            return;
        }

        for target in &mut plan.targets {
            if self.config.shadow.database_mode.enabled {
                if let Some(datasource) = self.config.shadow.database_mode.datasource.as_deref() {
                    target.datasource = datasource.to_string();
                }
            } else if !self.config.shadow.table_mode.enabled {
                target.datasource =
                    format!("{}{}", target.datasource, self.config.shadow.shadow_suffix);
            }

            if self.config.shadow.table_mode.enabled {
                for rewrite in &mut target.table_rewrites {
                    if self
                        .config
                        .shadow_routes_table(rewrite.logic_table.full_name().as_str())
                    {
                        rewrite.actual_table = self.shadow_table(&rewrite.actual_table);
                    }
                }
            }
        }
    }

    pub fn shadow_table(&self, table: &QualifiedTableName) -> QualifiedTableName {
        QualifiedTableName {
            schema: table.schema.clone(),
            table: format!("{}{}", table.table, self.config.shadow.shadow_suffix),
        }
    }
}

fn header_value<'a>(headers: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    headers
        .get(key)
        .or_else(|| headers.get(&key.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use sea_orm::{DbBackend, Statement};

    use crate::{
        config::ShardingConfig,
        connector::{ShardingHint, analyze_statement},
        shadow::ShadowRouter,
    };

    #[test]
    fn shadow_router_routes_by_hint_and_column_condition() {
        let config = Arc::new(
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
                  type = "column"
                  column = "is_shadow"
                  value = "1"
                "#,
            )
            .expect("config"),
        );
        let router = ShadowRouter::new(config);

        let mut analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log WHERE is_shadow = 1",
        ))
        .expect("analysis");
        assert!(router.should_route(&analysis));

        analysis.hint = Some(ShardingHint::Shadow);
        assert!(router.should_route_with_headers(&analysis, &BTreeMap::new()));
    }

    #[tokio::test]
    async fn shadow_router_routes_by_header_context() {
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [[shadow.conditions]]
                  type = "header"
                  key = "X-Shadow"
                  value = "true"
                "#,
            )
            .expect("config"),
        );
        let router = ShadowRouter::new(config);

        let mut analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log",
        ))
        .expect("analysis");

        assert!(!router.should_route(&analysis));

        analysis.shadow_headers = BTreeMap::from([("X-Shadow".to_string(), "true".to_string())]);
        assert!(router.should_route(&analysis));
    }
}
