#[cfg(feature = "web")]
use std::sync::Arc;

#[cfg(feature = "summer")]
use summer::app::AppBuilder;
#[cfg(feature = "summer")]
use summer::plugin::MutableComponentRegistry;

#[cfg(feature = "web")]
use crate::extensions::Extensions;
#[cfg(feature = "summer")]
use crate::registry::PluginRegistry;

#[cfg(feature = "web")]
pub type SqlRewriteRequestExtender =
    Arc<dyn Fn(&http::Extensions, &mut Extensions) + Send + Sync + 'static>;

#[cfg(feature = "summer")]
pub trait SqlRewriteConfigurator {
    fn sql_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry;
}

#[cfg(feature = "summer")]
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
