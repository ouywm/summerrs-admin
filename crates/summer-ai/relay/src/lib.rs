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
    fn relay_crate_exposes_openai_router_and_tracking_service_modules() {
        let _ = crate::router::openai::routes as fn() -> summer_web::Router;
        let _ = std::any::TypeId::of::<crate::service::tracking::TrackingService>();
        let _ = std::any::TypeId::of::<crate::service::chat::ChatRelayService>();
        let _ = std::any::TypeId::of::<crate::service::embeddings::EmbeddingsRelayService>();
        let _ = std::any::TypeId::of::<crate::service::responses::ResponsesRelayService>();
    }

    #[test]
    fn chat_route_delegates_stream_branching_to_service() {
        let source = include_str!("router/openai/chat.rs");
        assert!(!source.contains("if req.stream"));
        assert!(source.contains("svc.relay("));
    }

    #[test]
    fn responses_route_delegates_response_building_to_service() {
        let source = include_str!("router/openai/responses.rs");
        assert!(source.contains("svc.relay("));
        assert!(!source.contains("Json::<ResponsesResponse>"));
    }

    #[test]
    fn embeddings_route_delegates_response_building_to_service() {
        let source = include_str!("router/openai/embeddings.rs");
        assert!(source.contains("svc.relay("));
        assert!(!source.contains("Json::<EmbeddingResponse>"));
    }

    #[test]
    fn chat_service_keeps_stream_branching_inside_single_relay_method() {
        let source = include_str!("service/chat/mod.rs");
        assert!(!source.contains("async fn relay_chat_completion("));
        assert!(!source.contains("async fn relay_chat_completion_stream("));
        assert!(source.contains("if request.stream"));
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

    #[test]
    fn relay_crate_exposes_auth_chain_modules() {
        let _ = std::any::TypeId::of::<crate::auth::middleware::AiAuthLayer>();
        let _ = std::any::TypeId::of::<crate::auth::extractor::AiToken>();
        let _ = std::any::TypeId::of::<crate::service::token::TokenService>();
    }
}
