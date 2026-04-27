use summer_ai_model::dto::ability::{CreateAbilityDto, UpdateAbilityDto};
use summer_ai_model::entity::routing::ability;
use summer_ai_model::vo::ability::AbilityVo;

#[test]
fn create_ability_defaults_optional_fields() {
    let dto = CreateAbilityDto {
        channel_group: "default".into(),
        endpoint_scope: "chat".into(),
        model: "gpt-4o-mini".into(),
        channel_id: 9,
        enabled: None,
        priority: None,
        weight: None,
        route_config: None,
    };

    dto.validate_business_rules().expect("valid dto");
    let active = dto.into_active_model().expect("active model");

    assert_eq!(active.channel_group.unwrap(), "default");
    assert_eq!(active.endpoint_scope.unwrap(), "chat");
    assert_eq!(active.model.unwrap(), "gpt-4o-mini");
    assert_eq!(active.channel_id.unwrap(), 9);
    assert!(active.enabled.unwrap());
    assert_eq!(active.priority.unwrap(), 0);
    assert_eq!(active.weight.unwrap(), 100);
    assert_eq!(active.route_config.unwrap(), serde_json::json!({}));
}

#[test]
fn create_ability_rejects_invalid_channel_id() {
    let dto = CreateAbilityDto {
        channel_group: "default".into(),
        endpoint_scope: "chat".into(),
        model: "gpt-4o-mini".into(),
        channel_id: 0,
        enabled: None,
        priority: None,
        weight: None,
        route_config: None,
    };

    let err = dto.validate_business_rules().unwrap_err();
    assert!(err.contains("channelId"));
}

#[test]
fn update_ability_only_applies_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = ability::Model {
        id: 1,
        channel_group: "default".into(),
        endpoint_scope: "chat".into(),
        model: "gpt-4o-mini".into(),
        channel_id: 9,
        enabled: true,
        priority: 0,
        weight: 100,
        route_config: serde_json::json!({}),
        create_time: now,
        update_time: now,
    };
    let mut active: ability::ActiveModel = model.into();

    let dto = UpdateAbilityDto {
        channel_group: None,
        endpoint_scope: Some("responses".into()),
        model: Some("gpt-4.1".into()),
        channel_id: None,
        enabled: Some(false),
        priority: Some(20),
        weight: Some(300),
        route_config: Some(serde_json::json!({"tools": false})),
    };

    dto.validate_business_rules(&ability::Model {
        id: 1,
        channel_group: "default".into(),
        endpoint_scope: "chat".into(),
        model: "gpt-4o-mini".into(),
        channel_id: 9,
        enabled: true,
        priority: 0,
        weight: 100,
        route_config: serde_json::json!({}),
        create_time: now,
        update_time: now,
    })
    .expect("valid update");
    dto.apply_to(&mut active).expect("apply");

    assert_eq!(active.channel_group.unwrap(), "default");
    assert_eq!(active.endpoint_scope.unwrap(), "responses");
    assert_eq!(active.model.unwrap(), "gpt-4.1");
    assert_eq!(active.channel_id.unwrap(), 9);
    assert!(!active.enabled.unwrap());
    assert_eq!(active.priority.unwrap(), 20);
    assert_eq!(active.weight.unwrap(), 300);
    assert_eq!(
        active.route_config.unwrap(),
        serde_json::json!({"tools": false})
    );
}

#[test]
fn ability_vo_uses_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = AbilityVo::from_model(ability::Model {
        id: 1,
        channel_group: "premium".into(),
        endpoint_scope: "embeddings".into(),
        model: "text-embedding-3-large".into(),
        channel_id: 8,
        enabled: true,
        priority: 5,
        weight: 80,
        route_config: serde_json::json!({"probe": true}),
        create_time: now,
        update_time: now,
    });

    assert_eq!(vo.channel_id, 8);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
