//! summer-ai-admin
pub mod plugin;
pub mod router;
pub mod service;

pub use plugin::SummerAiAdminPlugin;

pub fn admin_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
