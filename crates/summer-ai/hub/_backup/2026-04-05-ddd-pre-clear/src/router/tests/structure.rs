#[test]
fn control_plane_routes_are_grouped_under_management_directory() {
    let router_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/router");
    let management_dir = router_dir.join("management");

    assert!(
        management_dir.is_dir(),
        "router/management directory should exist for control-plane route modules"
    );

    for file_name in [
        "alert.rs",
        "billing.rs",
        "channel.rs",
        "channel_account.rs",
        "channel_model_price.rs",
        "conversation.rs",
        "dashboard.rs",
        "file_storage.rs",
        "guardrail.rs",
        "log.rs",
        "model_config.rs",
        "multi_tenant.rs",
        "platform_config.rs",
        "request.rs",
        "runtime.rs",
        "token.rs",
        "vendor.rs",
    ] {
        assert!(
            !router_dir.join(file_name).exists(),
            "control-plane route file should live under router/management: {file_name}"
        );
    }

    for dir_name in ["channel", "config", "ops", "tenant"] {
        assert!(
            management_dir.join(dir_name).is_dir(),
            "router/management/{dir_name} directory should exist"
        );
    }
}

#[test]
fn router_root_keeps_tests_in_dedicated_directories() {
    let router_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/router");

    assert!(
        router_dir.join("tests").is_dir(),
        "router/tests directory should exist"
    );
    assert!(
        router_dir.join("tests/support").is_dir(),
        "router/tests/support directory should exist"
    );
    assert!(
        !router_dir.join("tests.rs").exists(),
        "router/tests.rs should be folded into router/tests/"
    );
    assert!(
        !router_dir.join("test_support.rs").exists(),
        "router/test_support.rs should be folded into router/tests/support/"
    );
}

#[test]
fn openai_router_uses_directory_module_layout() {
    let router_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/router");

    assert!(
        router_dir.join("openai/mod.rs").is_file(),
        "router/openai/mod.rs should exist"
    );
    assert!(
        !router_dir.join("openai.rs").exists(),
        "router/openai.rs should be folded into router/openai/mod.rs"
    );
}
