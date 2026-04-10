use summer_common::error::ApiResult;
use summer_web::axum::http::HeaderMap;

use super::RelayChatContext;
use crate::service::token::TokenInfo;

#[test]
fn relay_chat_context_keeps_request_metadata_together() -> ApiResult<()> {
    let ctx = RelayChatContext {
        token_info: TokenInfo {
            token_id: 1,
            user_id: 2,
            project_id: 3,
            service_account_id: 4,
            name: "demo".into(),
            group: "default".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: vec![],
            endpoint_scopes: vec![],
        },
        client_ip: "127.0.0.1".into(),
        user_agent: "codex-test".into(),
        request_headers: HeaderMap::new(),
    };

    assert_eq!(ctx.client_ip, "127.0.0.1");
    assert_eq!(ctx.user_agent, "codex-test");
    Ok(())
}
