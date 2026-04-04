#[test]
fn openai_relay_services_do_not_wrap_extractors_inside_service_methods() {
    let service_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/service");
    let files = [
        "openai_audio_speech_relay.rs",
        "openai_chat_relay.rs",
        "openai_completions_relay.rs",
        "openai_embeddings_relay.rs",
        "openai_images_relay.rs",
        "openai_moderations_relay.rs",
        "openai_rerank_relay.rs",
        "openai_responses_relay.rs",
    ];

    for file_name in files {
        let path = service_dir.join(file_name);
        let source =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
        assert!(
            !source.contains("AiToken(token_info),"),
            "{} should use plain service methods instead of re-wrapping extractors",
            path.display()
        );
        assert!(
            !source.contains("Component(self."),
            "{} should call internal logic directly instead of rebuilding Component wrappers",
            path.display()
        );
        assert!(
            !source.contains("ClientIp(client_ip)"),
            "{} should keep client ip as a plain parameter in service code",
            path.display()
        );
    }
}
