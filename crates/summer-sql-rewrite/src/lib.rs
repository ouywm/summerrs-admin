#![doc = include_str!("../README.md")]

pub mod builtin;
pub mod configurator;
pub mod connection;
pub mod context;
pub mod error;
pub mod extensions;
pub mod helpers;
pub mod pipeline;
pub mod plugin;
pub mod registry;
pub mod table;
pub mod transaction;
#[cfg(feature = "web")]
pub mod web;

pub use connection::RewriteConnection;
pub use context::{SqlOperation, SqlRewriteContext};
pub use error::{Result, SqlRewriteError};
pub use extensions::Extensions;
pub use plugin::SqlRewritePlugin;
pub use registry::PluginRegistry;
pub use table::QualifiedTableName;
pub use transaction::RewriteTransaction;

#[cfg(feature = "summer")]
pub use configurator::SqlRewriteConfigurator;
#[cfg(all(feature = "summer", feature = "web"))]
pub use configurator::SqlRewriteRequestExtender;

#[cfg(feature = "summer")]
use summer::app::AppBuilder;
#[cfg(feature = "summer")]
use summer::async_trait;
#[cfg(feature = "summer")]
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
#[cfg(all(feature = "summer", feature = "web"))]
use summer_web::LayerConfigurator;

#[cfg(feature = "summer")]
pub struct SummerSqlRewritePlugin;

#[cfg(feature = "summer")]
#[async_trait]
impl Plugin for SummerSqlRewritePlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db = app
            .get_component::<sea_orm::DatabaseConnection>()
            .expect("DatabaseConnection not found; ensure SeaOrmPlugin is registered first");

        let registry = app
            .get_component::<PluginRegistry>()
            .expect("PluginRegistry not found; ensure app component PluginRegistry");

        app.add_component(RewriteConnection::new(
            db.clone(),
            registry.clone(),
            Extensions::new(),
        ));

        #[cfg(feature = "web")]
        {
            let mut layer = web::SqlRewriteLayer::new(db.clone(), registry.clone());
            if let Some(extender) = app.get_component::<SqlRewriteRequestExtender>() {
                layer = layer.with_request_extender(extender.clone());
            }
            app.add_router_layer(move |router| router.layer(layer.clone()));
        }
    }

    fn name(&self) -> &str {
        "summer_sql_rewrite::SummerSqlRewritePlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}
