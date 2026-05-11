use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;

use crate::registry::PluginRegistry;

/// 把 `.sql_rewrite_configure(|reg| reg.register(...))` 注册的 [`PluginRegistry`]
/// 作为 component 挂进 app，供下游 plugin（如 `SummerShardingPlugin`）消费。
pub trait SqlRewriteConfigurator {
    fn sql_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry;
}

impl SqlRewriteConfigurator for AppBuilder {
    fn sql_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry,
    {
        let mut registry = PluginRegistry::new();
        f(&mut registry);
        if !registry.is_empty() {
            self.add_component(registry)
        } else {
            self
        }
    }
}
