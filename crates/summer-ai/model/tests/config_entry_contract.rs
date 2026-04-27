use summer_ai_model::dto::config_entry::{CreateConfigEntryDto, UpdateConfigEntryDto};
use summer_ai_model::entity::platform::config_entry::{self, ConfigEntryStatus};
use summer_ai_model::vo::config_entry::ConfigEntryVo;

#[test]
fn create_config_entry_defaults_optional_fields() {
    let dto = CreateConfigEntryDto {
        scope_type: "system".into(),
        scope_id: 0,
        category: "branding".into(),
        config_key: "site_name".into(),
        config_value: serde_json::json!({"value":"Summerrs"}),
        secret_ref: None,
        status: None,
        remark: None,
    };

    dto.validate_business_rules().expect("valid dto");
    let active = dto.into_active_model("operator").expect("active model");

    assert_eq!(active.scope_type.unwrap(), "system");
    assert_eq!(active.scope_id.unwrap(), 0);
    assert_eq!(active.secret_ref.unwrap(), "");
    assert_eq!(active.status.unwrap(), ConfigEntryStatus::Enabled);
    assert_eq!(active.version_no.unwrap(), 1);
    assert_eq!(active.remark.unwrap(), "");
}

#[test]
fn create_config_entry_rejects_nonzero_system_scope_id() {
    let dto = CreateConfigEntryDto {
        scope_type: "system".into(),
        scope_id: 1,
        category: "branding".into(),
        config_key: "site_name".into(),
        config_value: serde_json::json!({"value":"Summerrs"}),
        secret_ref: None,
        status: None,
        remark: None,
    };

    let err = dto.validate_business_rules().unwrap_err();
    assert!(err.contains("scopeId"));
}

#[test]
fn update_config_entry_only_applies_supplied_fields_and_bumps_version() {
    let now = chrono::Utc::now().fixed_offset();
    let model = config_entry::Model {
        id: 1,
        scope_type: "project".into(),
        scope_id: 8,
        category: "model".into(),
        config_key: "default_model".into(),
        config_value: serde_json::json!({"value":"gpt-4o-mini"}),
        secret_ref: String::new(),
        status: ConfigEntryStatus::Enabled,
        version_no: 3,
        remark: "default".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: config_entry::ActiveModel = model.into();

    let dto = UpdateConfigEntryDto {
        scope_type: None,
        scope_id: None,
        category: None,
        config_key: None,
        config_value: Some(serde_json::json!({"value":"gpt-4.1"})),
        secret_ref: Some("vault://system/default_model".into()),
        status: Some(ConfigEntryStatus::Disabled),
        remark: None,
    };

    dto.validate_business_rules(&config_entry::Model {
        id: 1,
        scope_type: "project".into(),
        scope_id: 8,
        category: "model".into(),
        config_key: "default_model".into(),
        config_value: serde_json::json!({"value":"gpt-4o-mini"}),
        secret_ref: String::new(),
        status: ConfigEntryStatus::Enabled,
        version_no: 3,
        remark: "default".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    })
    .expect("valid update");
    dto.apply_to(&mut active, "operator", 4).expect("apply");

    assert_eq!(active.scope_type.unwrap(), "project");
    assert_eq!(
        active.config_value.unwrap(),
        serde_json::json!({"value":"gpt-4.1"})
    );
    assert_eq!(active.secret_ref.unwrap(), "vault://system/default_model");
    assert_eq!(active.status.unwrap(), ConfigEntryStatus::Disabled);
    assert_eq!(active.version_no.unwrap(), 4);
    assert_eq!(active.remark.unwrap(), "default");
    assert_eq!(active.update_by.unwrap(), "operator");
}

#[test]
fn config_entry_vo_uses_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = ConfigEntryVo::from_model(config_entry::Model {
        id: 1,
        scope_type: "organization".into(),
        scope_id: 9,
        category: "quota".into(),
        config_key: "monthly_limit".into(),
        config_value: serde_json::json!({"value":1000}),
        secret_ref: String::new(),
        status: ConfigEntryStatus::Enabled,
        version_no: 2,
        remark: "tenant".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.scope_id, 9);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
