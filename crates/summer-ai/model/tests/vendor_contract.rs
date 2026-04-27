use summer_ai_model::dto::vendor::{CreateVendorDto, UpdateVendorDto};
use summer_ai_model::entity::routing::vendor::{self, ApiStyle};
use summer_ai_model::vo::vendor::VendorVo;

#[test]
fn create_vendor_defaults_optional_fields() {
    let dto = CreateVendorDto {
        vendor_code: "openai".into(),
        vendor_name: "OpenAI".into(),
        api_style: ApiStyle::OpenAiCompatible,
        icon: None,
        description: None,
        base_url: Some("https://api.openai.com/v1".into()),
        doc_url: None,
        metadata: None,
        vendor_sort: None,
        enabled: None,
        remark: None,
    };

    let active = dto.into_active_model("operator");
    assert_eq!(active.vendor_code.unwrap(), "openai");
    assert_eq!(active.enabled.unwrap(), true);
    assert_eq!(active.vendor_sort.unwrap(), 0);
    assert_eq!(active.metadata.unwrap(), serde_json::json!({}));
}

#[test]
fn update_vendor_only_applies_supplied_fields() {
    let now = chrono::Utc::now().fixed_offset();
    let model = vendor::Model {
        id: 1,
        vendor_code: "openai".into(),
        vendor_name: "OpenAI".into(),
        api_style: ApiStyle::OpenAiCompatible,
        icon: String::new(),
        description: String::new(),
        base_url: "https://api.openai.com/v1".into(),
        doc_url: String::new(),
        metadata: serde_json::json!({}),
        vendor_sort: 0,
        enabled: true,
        remark: String::new(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: vendor::ActiveModel = model.into();

    UpdateVendorDto {
        vendor_name: Some("OpenAI Official".into()),
        api_style: Some(ApiStyle::OpenAiCompatible),
        icon: Some("icon".into()),
        description: None,
        base_url: None,
        doc_url: Some("https://platform.openai.com/docs".into()),
        metadata: Some(serde_json::json!({"tier":"official"})),
        vendor_sort: Some(10),
        enabled: Some(false),
        remark: None,
    }
    .apply_to(&mut active, "operator");

    assert_eq!(active.vendor_name.unwrap(), "OpenAI Official");
    assert_eq!(active.icon.unwrap(), "icon");
    assert_eq!(active.doc_url.unwrap(), "https://platform.openai.com/docs");
    assert_eq!(active.vendor_sort.unwrap(), 10);
    assert_eq!(active.enabled.unwrap(), false);
}

#[test]
fn vendor_vo_preserves_api_style() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = VendorVo::from_model(vendor::Model {
        id: 1,
        vendor_code: "anthropic".into(),
        vendor_name: "Anthropic".into(),
        api_style: ApiStyle::AnthropicNative,
        icon: "icon".into(),
        description: "desc".into(),
        base_url: "https://api.anthropic.com".into(),
        doc_url: "https://docs.anthropic.com".into(),
        metadata: serde_json::json!({"official":true}),
        vendor_sort: 2,
        enabled: true,
        remark: "remark".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.vendor_code, "anthropic");
    assert_eq!(vo.api_style, ApiStyle::AnthropicNative);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
