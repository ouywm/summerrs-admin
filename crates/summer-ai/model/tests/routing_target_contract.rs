use summer_ai_model::dto::routing_target::{CreateRoutingTargetDto, UpdateRoutingTargetDto};
use summer_ai_model::entity::routing::routing_target::{self, RoutingTargetStatus};
use summer_ai_model::vo::routing_target::RoutingTargetVo;

#[test]
fn create_routing_target_defaults_optional_fields() {
    let dto = CreateRoutingTargetDto {
        routing_rule_id: 1,
        target_type: "channel".into(),
        channel_id: Some(9),
        account_id: None,
        plugin_id: None,
        target_key: None,
        weight: None,
        priority: None,
        cooldown_seconds: None,
        config: None,
        status: None,
    };

    dto.validate_business_rules().expect("valid dto");
    let active = dto.into_active_model().expect("active model");
    assert_eq!(active.channel_id.unwrap(), 9);
    assert_eq!(active.account_id.unwrap(), 0);
    assert_eq!(active.plugin_id.unwrap(), 0);
    assert_eq!(active.target_key.unwrap(), "");
    assert_eq!(active.weight.unwrap(), 100);
    assert_eq!(active.priority.unwrap(), 0);
    assert_eq!(active.cooldown_seconds.unwrap(), 0);
    assert_eq!(active.status.unwrap(), RoutingTargetStatus::Enabled);
    assert_eq!(active.config.unwrap(), serde_json::json!({}));
}

#[test]
fn routing_target_validation_rejects_missing_locator_for_target_type() {
    let dto = CreateRoutingTargetDto {
        routing_rule_id: 1,
        target_type: "channel_group".into(),
        channel_id: None,
        account_id: None,
        plugin_id: None,
        target_key: None,
        weight: None,
        priority: None,
        cooldown_seconds: None,
        config: None,
        status: None,
    };

    let err = dto.validate_business_rules().unwrap_err();
    assert!(err.contains("targetKey"));
}

#[test]
fn update_routing_target_only_applies_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = routing_target::Model {
        id: 1,
        routing_rule_id: 1,
        target_type: "channel".into(),
        channel_id: 9,
        account_id: 0,
        plugin_id: 0,
        target_key: String::new(),
        weight: 100,
        priority: 0,
        cooldown_seconds: 0,
        config: serde_json::json!({}),
        status: RoutingTargetStatus::Enabled,
        create_time: now,
        update_time: now,
    };
    let mut active: routing_target::ActiveModel = model.into();

    let dto = UpdateRoutingTargetDto {
        routing_rule_id: None,
        target_type: Some("pipeline".into()),
        channel_id: None,
        account_id: None,
        plugin_id: None,
        target_key: Some("smart-failover".into()),
        weight: Some(200),
        priority: Some(10),
        cooldown_seconds: Some(30),
        config: Some(serde_json::json!({"mode":"strict"})),
        status: Some(RoutingTargetStatus::Disabled),
    };

    dto.validate_business_rules(&routing_target::Model {
        id: 1,
        routing_rule_id: 1,
        target_type: "channel".into(),
        channel_id: 9,
        account_id: 0,
        plugin_id: 0,
        target_key: String::new(),
        weight: 100,
        priority: 0,
        cooldown_seconds: 0,
        config: serde_json::json!({}),
        status: RoutingTargetStatus::Enabled,
        create_time: now,
        update_time: now,
    })
    .expect("valid update");
    dto.apply_to(&mut active).expect("apply");

    assert_eq!(active.target_type.unwrap(), "pipeline");
    assert_eq!(active.channel_id.unwrap(), 0);
    assert_eq!(active.target_key.unwrap(), "smart-failover");
    assert_eq!(active.weight.unwrap(), 200);
    assert_eq!(active.priority.unwrap(), 10);
    assert_eq!(active.cooldown_seconds.unwrap(), 30);
    assert_eq!(active.config.unwrap(), serde_json::json!({"mode":"strict"}));
    assert_eq!(active.status.unwrap(), RoutingTargetStatus::Disabled);
}

#[test]
fn routing_target_vo_uses_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = RoutingTargetVo::from_model(routing_target::Model {
        id: 1,
        routing_rule_id: 1,
        target_type: "account".into(),
        channel_id: 9,
        account_id: 18,
        plugin_id: 0,
        target_key: String::new(),
        weight: 100,
        priority: 5,
        cooldown_seconds: 15,
        config: serde_json::json!({"sticky":true}),
        status: RoutingTargetStatus::Enabled,
        create_time: now,
        update_time: now,
    });

    assert_eq!(vo.account_id, 18);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
