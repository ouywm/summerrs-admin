use summer_ai_model::dto::openai_oauth::{
    ExchangeOpenAiOAuthCodeDto, GenerateOpenAiOAuthAuthUrlDto, RefreshOpenAiOAuthTokenDto,
};
use summer_ai_model::vo::openai_oauth::{
    OpenAiOAuthAuthUrlVo, OpenAiOAuthExchangeVo, OpenAiOAuthRefreshVo,
};

#[test]
fn generate_auth_url_dto_deserializes_camel_case() {
    let dto: GenerateOpenAiOAuthAuthUrlDto = serde_json::from_value(
        serde_json::json!({"redirectUri": "http://localhost:1455/auth/callback"}),
    )
    .expect("dto");

    assert_eq!(dto.redirect_uri, "http://localhost:1455/auth/callback");
}

#[test]
fn exchange_dto_supports_create_and_update_paths() {
    let create = ExchangeOpenAiOAuthCodeDto {
        session_id: "session-1".into(),
        code: "code-1".into(),
        state: "state-1".into(),
        channel_id: Some(1),
        account_id: None,
        name: Some("OpenAI OAuth".into()),
        remark: None,
        test_model: Some("gpt-4.1".into()),
    };
    assert!(create.validate_target().is_ok());

    let update = ExchangeOpenAiOAuthCodeDto {
        session_id: "session-1".into(),
        code: "code-1".into(),
        state: "state-1".into(),
        channel_id: None,
        account_id: Some(88),
        name: None,
        remark: None,
        test_model: None,
    };
    assert!(update.validate_target().is_ok());
}

#[test]
fn exchange_dto_rejects_invalid_target_combinations() {
    let missing_name = ExchangeOpenAiOAuthCodeDto {
        session_id: "session-1".into(),
        code: "code-1".into(),
        state: "state-1".into(),
        channel_id: Some(1),
        account_id: None,
        name: None,
        remark: None,
        test_model: None,
    };
    assert!(missing_name.validate_target().is_err());

    let missing_target = ExchangeOpenAiOAuthCodeDto {
        session_id: "session-1".into(),
        code: "code-1".into(),
        state: "state-1".into(),
        channel_id: None,
        account_id: None,
        name: None,
        remark: None,
        test_model: None,
    };
    assert!(missing_target.validate_target().is_err());

    let mixed_target = ExchangeOpenAiOAuthCodeDto {
        session_id: "session-1".into(),
        code: "code-1".into(),
        state: "state-1".into(),
        channel_id: Some(1),
        account_id: Some(88),
        name: Some("OpenAI OAuth".into()),
        remark: None,
        test_model: None,
    };
    assert!(mixed_target.validate_target().is_err());
}

#[test]
fn refresh_token_dto_deserializes_camel_case() {
    let dto: RefreshOpenAiOAuthTokenDto =
        serde_json::from_value(serde_json::json!({"accountId": 123})).expect("dto");

    assert_eq!(dto.account_id, 123);
}

#[test]
fn openai_oauth_vos_use_camel_case_and_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();

    let auth_url = OpenAiOAuthAuthUrlVo {
        auth_url: "https://auth.openai.com".into(),
        session_id: "session-1".into(),
    };
    let auth_url_json = serde_json::to_value(auth_url).expect("json");
    assert!(auth_url_json.get("authUrl").is_some());
    assert!(auth_url_json.get("sessionId").is_some());

    let exchange = OpenAiOAuthExchangeVo {
        account_id: 123,
        created: true,
        expires_at: now,
        subscription_expires_at: Some(now),
    };
    assert_eq!(exchange.expires_at, now);
    assert_eq!(exchange.subscription_expires_at, Some(now));
    let exchange_json = serde_json::to_value(&exchange).expect("json");
    assert!(exchange_json.get("accountId").is_some());
    assert!(exchange_json.get("expiresAt").is_some());
    assert!(exchange_json.get("subscriptionExpiresAt").is_some());

    let refresh = OpenAiOAuthRefreshVo {
        account_id: 123,
        refreshed_at: now,
        expires_at: now,
        subscription_expires_at: Some(now),
    };
    assert_eq!(refresh.refreshed_at, now);
    assert_eq!(refresh.expires_at, now);
    assert_eq!(refresh.subscription_expires_at, Some(now));
    let refresh_json = serde_json::to_value(&refresh).expect("json");
    assert!(refresh_json.get("accountId").is_some());
    assert!(refresh_json.get("refreshedAt").is_some());
    assert!(refresh_json.get("expiresAt").is_some());
    assert!(refresh_json.get("subscriptionExpiresAt").is_some());
}
