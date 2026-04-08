pub mod job;
pub mod plugin;
pub mod router;
pub mod service;

pub use plugin::SummerAiAdminPlugin;
pub use summer_ai_model::entity;

#[cfg(test)]
mod tests {
    #[test]
    fn admin_plugin_does_not_register_router_manually() {
        let source = include_str!("plugin.rs");
        assert!(!source.contains("add_router("));
    }

    #[test]
    fn admin_router_exposes_request_modules() {
        let source = include_str!("router/mod.rs");
        assert!(source.contains("pub mod request;"));
        assert!(source.contains("pub mod request_execution;"));
        assert!(source.contains("pub mod retry_attempt;"));
        assert!(source.contains("request::routes()"));
        assert!(source.contains("request_execution::routes()"));
        assert!(source.contains("retry_attempt::routes()"));
    }

    #[test]
    fn admin_service_exposes_request_modules() {
        let source = include_str!("service/mod.rs");
        assert!(source.contains("pub mod request;"));
        assert!(source.contains("pub mod request_execution;"));
        assert!(source.contains("pub mod retry_attempt;"));
    }
}
