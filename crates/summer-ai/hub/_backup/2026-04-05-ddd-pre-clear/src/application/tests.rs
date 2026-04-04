#[test]
fn openai_relay_services_do_not_wrap_extractors_inside_service_methods() {
    let service_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/application");
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

#[test]
fn hub_uses_ddd_roots_with_legacy_facades() {
    let src_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    assert!(
        src_dir.join("application/mod.rs").is_file(),
        "src/application/mod.rs should exist"
    );
    assert!(
        src_dir.join("infrastructure/mod.rs").is_file(),
        "src/infrastructure/mod.rs should exist"
    );
    assert!(
        src_dir.join("domain/mod.rs").is_file(),
        "src/domain/mod.rs should exist"
    );
    assert!(
        src_dir.join("interfaces/mod.rs").is_file(),
        "src/interfaces/mod.rs should exist"
    );
    assert!(
        src_dir.join("interfaces/http/mod.rs").is_file(),
        "src/interfaces/http/mod.rs should exist"
    );

    for facade in ["auth.rs", "job.rs", "relay.rs", "service.rs"] {
        assert!(
            src_dir.join(facade).is_file(),
            "legacy facade should exist: src/{facade}"
        );
    }

    for legacy_dir in ["auth", "job", "relay", "service"] {
        assert!(
            !src_dir.join(legacy_dir).is_dir(),
            "legacy implementation directory should be migrated away: src/{legacy_dir}"
        );
    }
}
