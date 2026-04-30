//! summer-ai-admin
pub mod plugin;
pub mod router;
pub mod service;

pub use plugin::SummerAiAdminPlugin;
pub use router::router_with_layers;

pub fn admin_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
