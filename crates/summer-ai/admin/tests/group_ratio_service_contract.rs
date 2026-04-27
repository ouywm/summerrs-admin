use summer_ai_admin::service::group_ratio_service::ensure_no_group_references;

#[test]
fn ensure_no_group_references_allows_zero_counts() {
    assert!(ensure_no_group_references(0, 0, 0, 0).is_ok());
}

#[test]
fn ensure_no_group_references_rejects_any_reference() {
    let err = ensure_no_group_references(1, 0, 2, 0).unwrap_err();
    assert!(err.contains("渠道=1"));
    assert!(err.contains("令牌=2"));
}
