use super::*;

#[test]
fn provider_kind_round_trips_channel_type() {
    assert_eq!(
        ProviderKind::from_channel_type(24),
        Some(ProviderKind::Gemini)
    );
    assert_eq!(ProviderKind::Gemini.channel_type(), 24);
    assert_eq!(ProviderKind::Gemini.display_name(), "Google Gemini");
    assert!(!ProviderKind::Gemini.is_openai_compatible());
    assert!(ProviderKind::from_channel_type(999).is_none());
}

#[test]
fn provider_registry_shares_openai_compatible_provider_instance() {
    let openai = ProviderRegistry::get(ProviderKind::OpenAi);
    let deepseek = ProviderRegistry::get(ProviderKind::DeepSeek);
    let openrouter = ProviderRegistry::get(ProviderKind::OpenRouter);

    assert!(std::ptr::eq(openai, deepseek));
    assert!(std::ptr::eq(openai, openrouter));
}

#[test]
fn provider_registry_reports_capabilities_from_metadata() {
    assert!(ProviderRegistry::chat(ProviderKind::Anthropic).is_some());
    assert!(ProviderRegistry::responses(ProviderKind::Anthropic).is_some());
    assert!(ProviderRegistry::embedding(ProviderKind::Anthropic).is_none());

    assert!(ProviderRegistry::embedding(ProviderKind::Gemini).is_some());
    assert!(ProviderRegistry::responses(ProviderKind::Gemini).is_some());

    assert!(ProviderRegistry::responses(ProviderKind::Groq).is_none());
    assert!(ProviderRegistry::embedding(ProviderKind::Groq).is_none());
}

#[test]
fn provider_registry_supported_scopes_use_kind_metadata() {
    assert_eq!(
        ProviderRegistry::supported_scopes(ProviderKind::Anthropic),
        &["chat", "responses"]
    );
    assert_eq!(
        ProviderRegistry::supported_scopes(ProviderKind::Gemini),
        &["chat", "responses", "embeddings"]
    );
    assert_eq!(
        ProviderRegistry::supported_scopes(ProviderKind::OpenAi),
        &["chat", "responses", "embeddings"]
    );
    assert_eq!(
        ProviderRegistry::supported_scopes(ProviderKind::DeepSeek),
        &["chat", "responses", "embeddings"]
    );
    assert_eq!(
        ProviderRegistry::supported_scopes(ProviderKind::Groq),
        &["chat"]
    );
}

#[test]
fn provider_registry_meta_returns_known_providers() {
    let openai = ProviderRegistry::meta(ProviderKind::OpenAi);
    assert_eq!(openai.name, "OpenAI");
    assert!(openai.openai_compatible);
    assert_eq!(
        openai.supported_scopes,
        &["chat", "responses", "embeddings"]
    );

    let anthropic = ProviderRegistry::meta(ProviderKind::Anthropic);
    assert_eq!(anthropic.name, "Anthropic");
    assert_eq!(anthropic.supported_scopes, &["chat", "responses"]);

    let groq = ProviderRegistry::meta(ProviderKind::Groq);
    assert_eq!(groq.name, "Groq");
    assert_eq!(groq.supported_scopes, &["chat"]);
}

#[test]
fn anthropic_and_gemini_responses_requests_bridge_to_chat_endpoints() {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "model": "demo",
        "input": "hello"
    });

    let anthropic = ProviderRegistry::responses(ProviderKind::Anthropic)
        .unwrap()
        .build_responses_request(
            &client,
            "https://api.anthropic.com",
            "sk-demo",
            &payload,
            "claude-sonnet-4",
        )
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        anthropic.url().as_str(),
        "https://api.anthropic.com/v1/messages"
    );

    let gemini = ProviderRegistry::responses(ProviderKind::Gemini)
        .unwrap()
        .build_responses_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "sk-demo",
            &payload,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        gemini.url().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
    );
}

#[test]
fn provider_parse_error_dispatches_to_specific_adapter() {
    let anthropic = ProviderRegistry::get(ProviderKind::Anthropic).parse_error(
        429,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"rate_limit_error","message":"too many requests"}}"#,
    );
    assert_eq!(anthropic.kind, ProviderErrorKind::RateLimit);

    let gemini = ProviderRegistry::get(ProviderKind::Gemini).parse_error(
        400,
        &HeaderMap::new(),
        br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad request"}}"#,
    );
    assert_eq!(gemini.kind, ProviderErrorKind::InvalidRequest);

    let openai = ProviderRegistry::get(ProviderKind::OpenAi).parse_error(
        401,
        &HeaderMap::new(),
        br#"{"error":{"message":"bad key","type":"invalid_request_error","code":"invalid_api_key"}}"#,
    );
    assert_eq!(openai.kind, ProviderErrorKind::Authentication);
}

#[test]
fn openai_compatible_html_error_is_echoed_verbatim() {
    let info = ProviderRegistry::get(ProviderKind::OpenAi).parse_error(
        403,
        &HeaderMap::new(),
        br#"<!DOCTYPE html><html><head><title>Attention Required! | Cloudflare</title></head><body>Sorry, you have been blocked</body></html>"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Authentication);
    assert_eq!(info.code, "authentication_error");
    assert_eq!(
        info.message,
        "<!DOCTYPE html><html><head><title>Attention Required! | Cloudflare</title></head><body>Sorry, you have been blocked</body></html>"
    );
}
