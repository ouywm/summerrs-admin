use std::sync::Arc;

use crate::error::{Result, ShardingError};

use super::{SqlRewritePlugin, context::RewriteContext};

/// 插件注册表。
///
/// 持有所有已注册的 `SqlRewritePlugin`，按 `order()` 升序排列。
/// 在 SQL Pipeline 中由 `DefaultSqlRewriter` 调用 `rewrite_all`。
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn SqlRewritePlugin>>,
}

impl PluginRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// 注册一个插件，返回 `&mut Self` 以支持链式调用。
    pub fn register(&mut self, plugin: impl SqlRewritePlugin) -> &mut Self {
        self.plugins.push(Arc::new(plugin));
        self.plugins.sort_by_key(|p| p.order());
        self
    }

    /// 批量注册
    pub fn register_all(&mut self, plugins: Vec<Arc<dyn SqlRewritePlugin>>) -> &mut Self {
        self.plugins.extend(plugins);
        self.plugins.sort_by_key(|p| p.order());
        self
    }

    /// 按优先级顺序执行所有匹配的插件
    pub fn rewrite_all(&self, ctx: &mut RewriteContext) -> Result<()> {
        for plugin in &self.plugins {
            if plugin.matches(ctx) {
                tracing::debug!(
                    plugin = plugin.name(),
                    order = plugin.order(),
                    "applying rewrite plugin"
                );
                plugin.rewrite(ctx).map_err(|e| ShardingError::Plugin {
                    plugin: plugin.name().to_string(),
                    message: e.to_string(),
                })?;
            }
        }
        Ok(())
    }

    /// 已注册的插件数量
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// 获取已注册插件的摘要信息（用于日志）
    pub fn summary(&self) -> String {
        self.plugins
            .iter()
            .map(|p| format!("{}(order={})", p.name(), p.order()))
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
