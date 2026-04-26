use summer_ai_core::oauth::SessionStore;
use summer_ai_core::oauth::openai::{
    OpenAiOAuthSession, build_authorization_url, generate_code_challenge, generate_code_verifier,
    generate_session_id, generate_state,
};

#[test]
fn build_authorization_url_includes_required_openai_flags() {
    let session_id = generate_session_id().expect("session id");
    assert!(!session_id.is_empty());

    let code_challenge = generate_code_challenge("challenge-1");
    assert!(!code_challenge.is_empty());

    let url = build_authorization_url(
        "state-1",
        &code_challenge,
        "http://localhost:1455/auth/callback",
        "app_test",
    )
    .expect("url");

    let raw = url.as_str();
    assert!(raw.contains("response_type=code"));
    assert!(raw.contains("client_id=app_test"));
    assert!(raw.contains("code_challenge_method=S256"));
    assert!(raw.contains("code_challenge="));
    assert!(raw.contains("id_token_add_organizations=true"));
    assert!(raw.contains("codex_cli_simplified_flow=true"));
}

#[tokio::test]
async fn session_store_expires_openai_sessions() {
    let store = SessionStore::new(std::time::Duration::from_millis(20));
    store
        .set(
            "session-1".into(),
            OpenAiOAuthSession {
                state: generate_state().unwrap(),
                code_verifier: generate_code_verifier().unwrap(),
                client_id: "app_test".into(),
                redirect_uri: "http://localhost:1455/auth/callback".into(),
                created_at: chrono::Utc::now(),
            },
        )
        .await;

    assert!(store.get("session-1").await.is_some());
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(store.get("session-1").await.is_none());
}
