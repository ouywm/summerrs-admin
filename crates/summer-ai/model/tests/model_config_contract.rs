use bigdecimal::BigDecimal;
use summer_ai_model::dto::model_config::{CreateModelConfigDto, UpdateModelConfigDto};
use summer_ai_model::entity::billing::model_config::{self, ModelConfigType};
use summer_ai_model::vo::model_config::ModelConfigVo;

#[test]
fn create_model_config_defaults_optional_fields() {
    let dto = CreateModelConfigDto {
        model_name: "gpt-4o-mini".into(),
        display_name: "GPT-4o Mini".into(),
        model_type: ModelConfigType::Chat,
        vendor_code: "openai".into(),
        supported_endpoints: None,
        input_ratio: None,
        output_ratio: None,
        cached_input_ratio: None,
        reasoning_ratio: None,
        capabilities: None,
        max_context: None,
        currency: None,
        effective_from: None,
        metadata: None,
        enabled: None,
        remark: None,
    };

    let active = dto.into_active_model("operator").expect("valid dto");
    assert_eq!(active.model_name.unwrap(), "gpt-4o-mini");
    assert_eq!(active.currency.unwrap(), "USD");
    assert!(active.enabled.unwrap());
    assert_eq!(active.input_ratio.unwrap(), BigDecimal::from(1));
    assert_eq!(active.output_ratio.unwrap(), BigDecimal::from(1));
    assert_eq!(active.cached_input_ratio.unwrap(), BigDecimal::from(0));
    assert_eq!(active.reasoning_ratio.unwrap(), BigDecimal::from(0));
    assert_eq!(active.supported_endpoints.unwrap(), serde_json::json!([]));
    assert_eq!(active.capabilities.unwrap(), serde_json::json!([]));
    assert_eq!(active.metadata.unwrap(), serde_json::json!({}));
}

#[test]
fn update_model_config_only_applies_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = model_config::Model {
        id: 1,
        model_name: "gpt-4o-mini".into(),
        display_name: "GPT-4o Mini".into(),
        model_type: ModelConfigType::Chat,
        vendor_code: "openai".into(),
        supported_endpoints: serde_json::json!(["chat/completions"]),
        input_ratio: BigDecimal::from(1),
        output_ratio: BigDecimal::from(1),
        cached_input_ratio: BigDecimal::from(0),
        reasoning_ratio: BigDecimal::from(0),
        capabilities: serde_json::json!(["text"]),
        max_context: 128000,
        currency: "USD".into(),
        effective_from: None,
        metadata: serde_json::json!({}),
        enabled: true,
        remark: "official".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: model_config::ActiveModel = model.into();

    UpdateModelConfigDto {
        display_name: Some("GPT-4o Mini Latest".into()),
        model_type: None,
        vendor_code: Some("openai-cn".into()),
        supported_endpoints: Some(vec!["chat/completions".into(), "responses".into()]),
        input_ratio: Some(1.25),
        output_ratio: None,
        cached_input_ratio: None,
        reasoning_ratio: Some(0.5),
        capabilities: Some(vec!["text".into(), "vision".into()]),
        max_context: Some(256000),
        currency: Some("usd".into()),
        effective_from: Some("2026-04-25T12:00:00+08:00".into()),
        metadata: Some(serde_json::json!({"tier":"latest"})),
        enabled: Some(false),
        remark: None,
    }
    .apply_to(&mut active, "operator")
    .expect("valid update dto");

    assert_eq!(active.display_name.unwrap(), "GPT-4o Mini Latest");
    assert_eq!(active.vendor_code.unwrap(), "openai-cn");
    assert_eq!(
        active.supported_endpoints.unwrap(),
        serde_json::json!(["chat/completions", "responses"])
    );
    assert_eq!(
        active.input_ratio.unwrap(),
        BigDecimal::from(125) / BigDecimal::from(100)
    );
    assert_eq!(
        active.reasoning_ratio.unwrap(),
        BigDecimal::from(5) / BigDecimal::from(10)
    );
    assert_eq!(active.max_context.unwrap(), 256000);
    assert_eq!(active.currency.unwrap(), "USD");
    assert!(!active.enabled.unwrap());
    assert_eq!(active.remark.unwrap(), "official");
}

#[test]
fn model_config_vo_expands_json_and_effective_from() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = ModelConfigVo::from_model(model_config::Model {
        id: 1,
        model_name: "claude-3-7-sonnet".into(),
        display_name: "Claude 3.7 Sonnet".into(),
        model_type: ModelConfigType::Reasoning,
        vendor_code: "anthropic".into(),
        supported_endpoints: serde_json::json!(["messages", "responses"]),
        input_ratio: BigDecimal::from(3),
        output_ratio: BigDecimal::from(15),
        cached_input_ratio: BigDecimal::from(1),
        reasoning_ratio: BigDecimal::from(2),
        capabilities: serde_json::json!(["vision", "tool_call"]),
        max_context: 200000,
        currency: "USD".into(),
        effective_from: Some(now),
        metadata: serde_json::json!({"family":"claude"}),
        enabled: true,
        remark: "official".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });
    assert_eq!(vo.model_name, "claude-3-7-sonnet");
    assert_eq!(vo.model_type, ModelConfigType::Reasoning);
    assert_eq!(vo.supported_endpoints, vec!["messages", "responses"]);
    assert_eq!(vo.capabilities, vec!["vision", "tool_call"]);
    assert_eq!(vo.effective_from, Some(now));
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
