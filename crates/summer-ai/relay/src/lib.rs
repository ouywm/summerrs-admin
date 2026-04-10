pub mod auth;
pub mod job;
pub mod plugin;
pub mod router;
pub mod service;

pub use plugin::SummerAiRelayPlugin;
pub use summer_ai_model::entity;

#[cfg(test)]
mod tests {
    #[test]
    fn relay_job_exposes_daily_stats_module() {
        let source = include_str!("job/mod.rs");
        assert!(source.contains("mod daily_stats;") || source.contains("pub mod daily_stats;"));
    }

    #[test]
    fn relay_service_exposes_daily_stats_module() {
        let source = include_str!("service/mod.rs");
        assert!(source.contains("pub mod daily_stats;"));
    }

    #[test]
    fn relay_job_exposes_alert_module() {
        let source = include_str!("job/mod.rs");
        assert!(source.contains("mod alert;") || source.contains("pub mod alert;"));
    }

    #[test]
    fn relay_service_exposes_alert_module() {
        let source = include_str!("service/mod.rs");
        assert!(source.contains("pub mod alert;"));
    }

    #[test]
    fn relay_plugin_registers_stream_task_shutdown_hook() {
        let source = include_str!("plugin.rs");
        assert!(source.contains("RelayStreamTaskTracker"));
        assert!(source.contains("add_shutdown_hook"));
        assert!(source.contains("timeout("));
    }

    #[test]
    fn chat_stream_tracking_avoids_detached_tokio_spawn() {
        let source = include_str!("service/chat/stream.rs");
        assert!(!source.contains("tokio::spawn("));
    }
}
