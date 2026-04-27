use summer_ai_admin::service::model_config_service::ensure_no_model_config_references;

#[test]
fn ensure_no_model_config_references_allows_zero_counts() {
    assert!(ensure_no_model_config_references(0, 0, 0).is_ok());
}

#[test]
fn ensure_no_model_config_references_rejects_nonzero_counts() {
    let err = ensure_no_model_config_references(2, 1, 3).unwrap_err();
    assert!(err.contains("渠道=2"));
    assert!(err.contains("渠道账号=1"));
    assert!(err.contains("渠道价格=3"));
}
