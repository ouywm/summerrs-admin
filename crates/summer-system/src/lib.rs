pub mod job;
pub mod plugins;
pub mod resource_permission;
pub mod router;
pub mod service;
pub mod socketio;

pub use router::router_with_layers;

pub fn system_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
