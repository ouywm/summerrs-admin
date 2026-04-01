use std::sync::Arc;

use crate::{
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
    use crate::{SqlRewriteContext, SqlRewriteError, SqlRewritePlugin};

    use super::PluginRegistry;

    struct AlreadyWrappedPlugin;

    impl SqlRewritePlugin for AlreadyWrappedPlugin {
        fn name(&self) -> &str {
            "wrapped"
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, _ctx: &mut SqlRewriteContext) -> crate::Result<()> {
            Err(SqlRewriteError::Plugin {
                plugin: "wrapped".to_string(),
                message: "boom".to_string(),
            })
        }
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
        let mut ext = crate::Extensions::new();
        let mut ctx = SqlRewriteContext {
            statement: &mut stmt,
            operation: crate::SqlOperation::Select,
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
}
