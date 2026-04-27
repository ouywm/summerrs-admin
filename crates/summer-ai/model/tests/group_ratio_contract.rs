use summer_ai_model::dto::group_ratio::{CreateGroupRatioDto, UpdateGroupRatioDto};
use summer_ai_model::entity::billing::group_ratio;
use summer_ai_model::vo::group_ratio::GroupRatioVo;

#[test]
fn create_group_ratio_defaults_json_fields() {
    let dto = CreateGroupRatioDto {
        group_code: "vip".into(),
        group_name: "VIP".into(),
        ratio: 1.5,
        enabled: None,
        model_whitelist: Some(vec!["gpt-4o".into(), "".into()]),
        model_blacklist: None,
        endpoint_scopes: Some(vec!["chat".into(), "responses".into()]),
        fallback_group_code: None,
        policy: None,
        remark: Some("tier".into()),
    };

    let active = dto.into_active_model("operator");
    assert_eq!(active.group_code.unwrap(), "vip");
    assert_eq!(active.enabled.unwrap(), true);
    assert_eq!(
        active.model_whitelist.unwrap(),
        serde_json::json!(["gpt-4o"])
    );
    assert_eq!(
        active.endpoint_scopes.unwrap(),
        serde_json::json!(["chat", "responses"])
    );
    assert_eq!(active.fallback_group_code.unwrap(), "");
}

#[test]
fn update_group_ratio_only_touches_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = group_ratio::Model {
        id: 1,
        group_code: "default".into(),
        group_name: "Default".into(),
        ratio: bigdecimal::BigDecimal::from(1),
        enabled: true,
        model_whitelist: serde_json::json!(["gpt-4o"]),
        model_blacklist: serde_json::json!([]),
        endpoint_scopes: serde_json::json!(["chat"]),
        fallback_group_code: String::new(),
        policy: serde_json::json!({}),
        remark: "old".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: group_ratio::ActiveModel = model.into();

    UpdateGroupRatioDto {
        group_name: Some("VIP".into()),
        ratio: Some(2.0),
        enabled: Some(false),
        model_whitelist: None,
        model_blacklist: Some(vec!["bad-model".into()]),
        endpoint_scopes: None,
        fallback_group_code: Some("backup".into()),
        policy: Some(serde_json::json!({"route":"fixed"})),
        remark: None,
    }
    .apply_to(&mut active, "operator");

    assert_eq!(active.group_name.unwrap(), "VIP");
    assert_eq!(active.enabled.unwrap(), false);
    assert_eq!(
        active.model_whitelist.unwrap(),
        serde_json::json!(["gpt-4o"])
    );
    assert_eq!(
        active.model_blacklist.unwrap(),
        serde_json::json!(["bad-model"])
    );
    assert_eq!(active.fallback_group_code.unwrap(), "backup");
    assert_eq!(active.policy.unwrap(), serde_json::json!({"route":"fixed"}));
}

#[test]
fn group_ratio_vo_converts_decimal_ratio() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = GroupRatioVo::from_model(group_ratio::Model {
        id: 1,
        group_code: "vip".into(),
        group_name: "VIP".into(),
        ratio: bigdecimal::BigDecimal::try_from(1.25).unwrap(),
        enabled: true,
        model_whitelist: serde_json::json!(["gpt-4o"]),
        model_blacklist: serde_json::json!(["bad"]),
        endpoint_scopes: serde_json::json!(["chat"]),
        fallback_group_code: "backup".into(),
        policy: serde_json::json!({"route":"fixed"}),
        remark: "remark".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.ratio, 1.25);
    assert_eq!(vo.group_code, "vip");
    assert_eq!(vo.model_blacklist, vec!["bad"]);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
