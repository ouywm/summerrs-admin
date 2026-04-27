use summer_ai_admin::service::vendor_service::ensure_no_vendor_references;

#[test]
fn ensure_no_vendor_references_allows_zero_counts() {
    assert!(ensure_no_vendor_references(0, 0).is_ok());
}

#[test]
fn ensure_no_vendor_references_rejects_nonzero_counts() {
    let err = ensure_no_vendor_references(2, 1).unwrap_err();
    assert!(err.contains("渠道=2"));
    assert!(err.contains("模型配置=1"));
}
