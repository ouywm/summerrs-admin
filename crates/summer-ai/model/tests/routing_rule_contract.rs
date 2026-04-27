use summer_ai_model::dto::routing_rule::{CreateRoutingRuleDto, UpdateRoutingRuleDto};
use summer_ai_model::entity::routing::routing_rule::{self, RoutingRuleStatus};
use summer_ai_model::vo::routing_rule::RoutingRuleVo;

#[test]
fn create_routing_rule_defaults_optional_fields() {
    let dto = CreateRoutingRuleDto {
        organization_id: 1,
        project_id: 2,
        rule_code: "tenant-default".into(),
        rule_name: "Tenant Default".into(),
        priority: None,
        match_type: "model".into(),
        match_conditions: None,
        route_strategy: "priority".into(),
        fallback_strategy: None,
        status: None,
        start_time: None,
        end_time: None,
        metadata: None,
        remark: None,
    };

    let active = dto.into_active_model("operator").expect("valid dto");
    assert_eq!(active.priority.unwrap(), 0);
    assert_eq!(active.fallback_strategy.unwrap(), "none");
    assert_eq!(active.status.unwrap(), RoutingRuleStatus::Enabled);
    assert_eq!(active.match_conditions.unwrap(), serde_json::json!({}));
    assert_eq!(active.metadata.unwrap(), serde_json::json!({}));
}

#[test]
fn update_routing_rule_only_applies_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = routing_rule::Model {
        id: 1,
        organization_id: 1,
        project_id: 2,
        rule_code: "tenant-default".into(),
        rule_name: "Tenant Default".into(),
        priority: 0,
        match_type: "model".into(),
        match_conditions: serde_json::json!({"model":"gpt-4o-mini"}),
        route_strategy: "priority".into(),
        fallback_strategy: "none".into(),
        status: RoutingRuleStatus::Enabled,
        start_time: None,
        end_time: None,
        metadata: serde_json::json!({}),
        remark: "default".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: routing_rule::ActiveModel = model.into();

    UpdateRoutingRuleDto {
        organization_id: None,
        project_id: None,
        rule_code: None,
        rule_name: Some("Tenant Preferred".into()),
        priority: Some(100),
        match_type: None,
        match_conditions: Some(serde_json::json!({"model":"gpt-4.1"})),
        route_strategy: Some("weighted".into()),
        fallback_strategy: Some("failover".into()),
        status: Some(RoutingRuleStatus::Disabled),
        start_time: Some("2026-04-26T09:00:00+08:00".into()),
        end_time: None,
        metadata: Some(serde_json::json!({"env":"prod"})),
        remark: None,
    }
    .apply_to(&mut active, "operator")
    .expect("valid update");

    assert_eq!(active.rule_name.unwrap(), "Tenant Preferred");
    assert_eq!(active.priority.unwrap(), 100);
    assert_eq!(active.route_strategy.unwrap(), "weighted");
    assert_eq!(active.fallback_strategy.unwrap(), "failover");
    assert_eq!(active.status.unwrap(), RoutingRuleStatus::Disabled);
    assert_eq!(active.metadata.unwrap(), serde_json::json!({"env":"prod"}));
    assert_eq!(active.remark.unwrap(), "default");
}

#[test]
fn routing_rule_vo_keeps_schedule_window() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = RoutingRuleVo::from_model(routing_rule::Model {
        id: 1,
        organization_id: 1,
        project_id: 2,
        rule_code: "tenant-default".into(),
        rule_name: "Tenant Default".into(),
        priority: 100,
        match_type: "model".into(),
        match_conditions: serde_json::json!({"model":"claude-3-7-sonnet"}),
        route_strategy: "weighted".into(),
        fallback_strategy: "failover".into(),
        status: RoutingRuleStatus::Enabled,
        start_time: Some(now),
        end_time: Some(now),
        metadata: serde_json::json!({"env":"prod"}),
        remark: "default".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });
    assert_eq!(vo.rule_code, "tenant-default");
    assert_eq!(vo.status, RoutingRuleStatus::Enabled);
    assert_eq!(vo.start_time, Some(now));
    assert_eq!(vo.end_time, Some(now));
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
