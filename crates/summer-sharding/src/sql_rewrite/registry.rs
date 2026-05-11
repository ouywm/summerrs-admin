use std::sync::Arc;

use crate::sql_rewrite::{
    QualifiedTableName,
    context::SqlRewriteContext,
    error::{Result, SqlRewriteError},
    plugin::SqlRewritePlugin,
};

pub struct PluginRegistry {
    plugins: Vec<Arc<dyn SqlRewritePlugin>>,
}

const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}

    let _ = assert_send_sync::<PluginRegistry>;
};

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: impl SqlRewritePlugin) -> &mut Self {
        let plugin: Arc<dyn SqlRewritePlugin> = Arc::new(plugin);
        let insert_at = self
            .plugins
            .binary_search_by_key(&plugin.order(), |item| item.order())
            .unwrap_or_else(|idx| idx);
        self.plugins.insert(insert_at, plugin);
        self
    }

    pub fn register_all(&mut self, plugins: Vec<Arc<dyn SqlRewritePlugin>>) -> &mut Self {
        self.plugins.extend(plugins);
        self.plugins.sort_by_key(|plugin| plugin.order());
        self
    }

    pub fn rewrite_all(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        for plugin in &self.plugins {
            if !table_filter_passes(plugin.as_ref(), &ctx.tables) {
                continue;
            }
            if plugin.matches(ctx) {
                tracing::debug!(
                    plugin = plugin.name(),
                    order = plugin.order(),
                    "applying sql rewrite plugin"
                );
                plugin.rewrite(ctx).map_err(|error| match error {
                    SqlRewriteError::Plugin { .. } => error,
                    other => SqlRewriteError::Plugin {
                        plugin: plugin.name().to_string(),
                        message: other.to_string(),
                    },
                })?;
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn summary(&self) -> String {
        self.plugins
            .iter()
            .map(|plugin| format!("{}(order={})", plugin.name(), plugin.order()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// 框架层表名过滤：在调用 plugin.matches() 之前执行。
/// - skip_tables 优先：命中任意一个就跳过
/// - tables 为空：全部表通过
/// - tables 非空：至少命中一个才通过
fn table_filter_passes(plugin: &dyn SqlRewritePlugin, tables: &[String]) -> bool {
    let skip = plugin.skip_tables();
    let allow = plugin.tables();
    if skip.is_empty() && allow.is_empty() {
        return true;
    }
    tables.iter().any(|t| {
        let candidate = QualifiedTableName::parse(t);
        if skip.iter().any(|s| s.matches_qualified(&candidate)) {
            return false;
        }
        allow.is_empty() || allow.iter().any(|a| a.matches_qualified(&candidate))
    })
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
        }
    }
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("plugins", &self.summary())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::sql_rewrite::{
        QualifiedTableName, SqlRewriteContext, SqlRewriteError, SqlRewritePlugin,
    };

    use super::PluginRegistry;

    struct AlreadyWrappedPlugin;

    impl SqlRewritePlugin for AlreadyWrappedPlugin {
        fn name(&self) -> &str {
            "wrapped"
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, _ctx: &mut SqlRewriteContext) -> crate::sql_rewrite::Result<()> {
            Err(SqlRewriteError::Plugin {
                plugin: "wrapped".to_string(),
                message: "boom".to_string(),
            })
        }
    }

    /// 限定表名的插件，用于验证框架层 table_filter_passes。
    struct ScopedPlugin {
        name: &'static str,
        tables: Vec<QualifiedTableName>,
        skip_tables: Vec<QualifiedTableName>,
        hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl SqlRewritePlugin for ScopedPlugin {
        fn name(&self) -> &str {
            self.name
        }

        fn tables(&self) -> &[QualifiedTableName] {
            &self.tables
        }

        fn skip_tables(&self) -> &[QualifiedTableName] {
            &self.skip_tables
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, _ctx: &mut SqlRewriteContext) -> crate::sql_rewrite::Result<()> {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
    }

    fn run_with_table(registry: &PluginRegistry, table: &str) {
        let mut stmt = sqlparser::parser::Parser::parse_sql(
            &sqlparser::dialect::PostgreSqlDialect {},
            &format!("SELECT * FROM {table}"),
        )
        .expect("parse")
        .remove(0);
        let mut ext = crate::sql_rewrite::Extensions::new();
        let mut ctx = SqlRewriteContext {
            statement: &mut stmt,
            operation: crate::sql_rewrite::SqlOperation::Select,
            tables: vec![table.to_string()],
            original_sql: "",
            extensions: &mut ext,
            comments: Vec::new(),
        };
        registry.rewrite_all(&mut ctx).expect("rewrite");
    }

    #[test]
    fn rewrite_all_keeps_existing_plugin_error_shape() {
        let mut registry = PluginRegistry::new();
        registry.register(AlreadyWrappedPlugin);
        let mut stmt = sqlparser::parser::Parser::parse_sql(
            &sqlparser::dialect::PostgreSqlDialect {},
            "SELECT * FROM users",
        )
        .expect("parse")
        .remove(0);
        let mut ext = crate::sql_rewrite::Extensions::new();
        let mut ctx = SqlRewriteContext {
            statement: &mut stmt,
            operation: crate::sql_rewrite::SqlOperation::Select,
            tables: vec!["users".to_string()],
            original_sql: "SELECT * FROM users",
            extensions: &mut ext,
            comments: Vec::new(),
        };

        let error = registry
            .rewrite_all(&mut ctx)
            .expect_err("plugin should fail");

        match error {
            SqlRewriteError::Plugin { plugin, message } => {
                assert_eq!(plugin, "wrapped");
                assert_eq!(message, "boom");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn whitelist_filter_runs_plugin_only_for_listed_table() {
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry.register(ScopedPlugin {
            name: "scoped",
            tables: vec![QualifiedTableName::parse("sys.user")],
            skip_tables: vec![],
            hits: hits.clone(),
        });

        run_with_table(&registry, "sys.user");
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 1);

        run_with_table(&registry, "sys.role");
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "non-whitelisted table should not invoke plugin"
        );
    }

    #[test]
    fn skip_filter_takes_priority_over_whitelist() {
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry.register(ScopedPlugin {
            name: "scoped",
            tables: vec![QualifiedTableName::parse("sys.user")],
            skip_tables: vec![QualifiedTableName::parse("sys.user")],
            hits: hits.clone(),
        });

        run_with_table(&registry, "sys.user");
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "skip_tables should override tables whitelist"
        );
    }

    #[test]
    fn empty_filters_run_for_every_table() {
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry.register(ScopedPlugin {
            name: "scoped",
            tables: vec![],
            skip_tables: vec![],
            hits: hits.clone(),
        });

        run_with_table(&registry, "sys.user");
        run_with_table(&registry, "sys.role");
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn schema_less_pattern_matches_table_in_any_schema() {
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry.register(ScopedPlugin {
            name: "scoped",
            tables: vec![QualifiedTableName::parse("user")],
            skip_tables: vec![],
            hits: hits.clone(),
        });

        run_with_table(&registry, "sys.user");
        run_with_table(&registry, "biz.user");
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "schema-less pattern should match any schema"
        );

        run_with_table(&registry, "sys.role");
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "table name mismatch should not invoke plugin"
        );
    }
}
