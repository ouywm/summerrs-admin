use summer_ai_admin::service::routing_rule_service::ensure_no_routing_rule_targets;

#[test]
fn ensure_no_routing_rule_targets_allows_zero_count() {
    assert!(ensure_no_routing_rule_targets(0).is_ok());
}

#[test]
fn ensure_no_routing_rule_targets_rejects_existing_targets() {
    let err = ensure_no_routing_rule_targets(3).unwrap_err();
    assert!(err.contains("路由目标=3"));
}
