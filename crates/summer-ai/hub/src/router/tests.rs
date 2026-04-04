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
}
