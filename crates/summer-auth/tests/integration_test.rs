use std::sync::Arc;

use summer_auth::config::AuthConfig;
use summer_auth::error::AuthError;
use summer_auth::online::OnlineUserQuery;
use summer_auth::qrcode::QrCodeState;
use summer_auth::session::{SessionManager, permission_matches};
use summer_auth::storage::memory::MemoryStorage;
use summer_auth::user_type::{DeviceType, LoginId, UserType};
use summer_auth::{AdminProfile, BusinessProfile, CustomerProfile, UserProfile};

fn default_config() -> AuthConfig {
    serde_json::from_str(
        r#"{
            "token_name": "Authorization",
            "access_timeout": 3600,
            "refresh_timeout": 86400,
            "concurrent_login": true,
            "max_devices": 3,
            "qr_code_timeout": 300,
            "jwt_secret": "test-jwt-secret-key-for-integration-tests-32chars!"
        }"#,
    )
    .unwrap()
}

fn make_manager() -> SessionManager {
    let storage = Arc::new(MemoryStorage::new());
    SessionManager::new(storage, default_config())
}

fn admin_login_params(user_id: i64) -> summer_auth::session::LoginParams {
    summer_auth::session::LoginParams {
        login_id: LoginId::admin(user_id),
        device: DeviceType::Web,
        login_ip: "127.0.0.1".to_string(),
        user_agent: "test-agent".to_string(),
        tenant_id: None,
        profile: UserProfile::Admin(AdminProfile {
            user_name: "test_user".to_string(),
            nick_name: "Test User".to_string(),
            roles: vec!["admin".to_string()],
            permissions: vec![
                "system:user:list".to_string(),
                "system:user:add".to_string(),
            ],
        }),
    }
}

fn biz_login_params(user_id: i64) -> summer_auth::session::LoginParams {
    summer_auth::session::LoginParams {
        login_id: LoginId::business(user_id),
        device: DeviceType::Web,
        login_ip: "192.168.1.1".to_string(),
        user_agent: "biz-agent".to_string(),
        tenant_id: None,
        profile: UserProfile::Business(BusinessProfile {
            user_name: "biz_user".to_string(),
            nick_name: "Biz User".to_string(),
            roles: vec!["merchant".to_string()],
            permissions: vec!["order:list".to_string()],
        }),
    }
}

fn customer_login_params(user_id: i64) -> summer_auth::session::LoginParams {
    summer_auth::session::LoginParams {
        login_id: LoginId::customer(user_id),
        device: DeviceType::Web,
        login_ip: "10.0.0.1".to_string(),
        user_agent: "customer-agent".to_string(),
        tenant_id: None,
        profile: UserProfile::Customer(CustomerProfile {
            nick_name: "Customer".to_string(),
        }),
    }
}

fn admin_profile() -> UserProfile {
    UserProfile::Admin(AdminProfile {
        user_name: "test_user".to_string(),
        nick_name: "Test User".to_string(),
        roles: vec!["admin".to_string()],
        permissions: vec![
            "system:user:list".to_string(),
            "system:user:add".to_string(),
        ],
    })
}

// ── 登录/登出 ──

#[tokio::test]
async fn login_returns_token_pair() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    assert!(!pair.access_token.is_empty());
    assert!(!pair.refresh_token.is_empty());
    assert_eq!(pair.expires_in, 3600);
    // JWT 格式: 3 段 base64 用 . 分隔
    assert_eq!(pair.access_token.split('.').count(), 3);
    assert_eq!(pair.refresh_token.split('.').count(), 3);
}

#[tokio::test]
async fn validate_token_returns_user_info() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    let validated = mgr.validate_token(&pair.access_token).await.unwrap();

    assert_eq!(validated.login_id, LoginId::admin(1));
    assert_eq!(validated.device, DeviceType::Web);
    assert_eq!(validated.user_name, "test_user");
    assert_eq!(validated.nick_name, "Test User");
    assert_eq!(validated.roles, vec!["admin"]);
    assert_eq!(
        validated.permissions,
        vec!["system:user:list", "system:user:add"]
    );
}

#[tokio::test]
async fn validate_token_preserves_tenant_id() {
    let mgr = make_manager();
    let mut params = admin_login_params(1);
    params.tenant_id = Some("T-001".to_string());

    let pair = mgr.login(params).await.unwrap();
    let validated = mgr.validate_token(&pair.access_token).await.unwrap();

    assert_eq!(validated.tenant_id.as_deref(), Some("T-001"));
}

#[tokio::test]
async fn validate_invalid_token_fails() {
    let mgr = make_manager();
    let result = mgr.validate_token("not.a.valid-jwt").await;
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

#[tokio::test]
async fn logout_sets_deny_key() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.logout(&LoginId::admin(1), &DeviceType::Web)
        .await
        .unwrap();

    // deny key = "refresh:{ts}" → RefreshRequired（旧 token iat <= deny_ts）
    let result = mgr.validate_token(&pair.access_token).await;
    assert!(matches!(result, Err(AuthError::RefreshRequired)));
}

#[tokio::test]
async fn logout_all_invalidates_all_devices() {
    let mgr = make_manager();

    let mut params1 = admin_login_params(1);
    params1.device = DeviceType::Web;
    let pair1 = mgr.login(params1).await.unwrap();

    let mut params2 = admin_login_params(1);
    params2.device = DeviceType::Android;
    let pair2 = mgr.login(params2).await.unwrap();

    mgr.logout_all(&LoginId::admin(1)).await.unwrap();

    assert!(mgr.validate_token(&pair1.access_token).await.is_err());
    assert!(mgr.validate_token(&pair2.access_token).await.is_err());
}

#[tokio::test]
async fn refresh_preserves_tenant_id() {
    let mgr = make_manager();
    let mut params = admin_login_params(1);
    params.tenant_id = Some("T-REFRESH-001".to_string());

    let pair = mgr.login(params).await.unwrap();
    let refreshed = mgr
        .refresh(
            &pair.refresh_token,
            &admin_profile(),
            Some("T-REFRESH-001"),
        )
        .await
        .unwrap();
    let validated = mgr.validate_token(&refreshed.access_token).await.unwrap();

    assert_eq!(validated.tenant_id.as_deref(), Some("T-REFRESH-001"));
}

// ── 多设备 ──

#[tokio::test]
async fn concurrent_login_multiple_devices() {
    let mgr = make_manager();

    let mut p1 = admin_login_params(1);
    p1.device = DeviceType::Web;
    let pair1 = mgr.login(p1).await.unwrap();

    let mut p2 = admin_login_params(1);
    p2.device = DeviceType::Android;
    let pair2 = mgr.login(p2).await.unwrap();

    // 两个 token 都有效
    assert!(mgr.validate_token(&pair1.access_token).await.is_ok());
    assert!(mgr.validate_token(&pair2.access_token).await.is_ok());

    // 两个设备都在线
    let devices = mgr.get_devices(&LoginId::admin(1)).await.unwrap();
    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn max_devices_evicts_oldest() {
    let mgr = make_manager(); // max_devices = 3

    let devices = [
        DeviceType::Web,
        DeviceType::Android,
        DeviceType::IOS,
        DeviceType::Desktop,
    ];
    let mut tokens = Vec::new();

    for d in &devices {
        let mut p = admin_login_params(1);
        p.device = d.clone();
        let pair = mgr.login(p).await.unwrap();
        tokens.push(pair);
        // 确保 login_time 递增（毫秒精度）
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Stateless: access JWT 仍然有效（没有 deny key）
    // 但设备列表只剩 3 个，且 Web 的 refresh key 被删
    let device_list = mgr.get_devices(&LoginId::admin(1)).await.unwrap();
    assert_eq!(device_list.len(), 3);

    // 旧 refresh token（Web）不能使用
    assert!(
        mgr.refresh(&tokens[0].refresh_token, &admin_profile(), None)
            .await
            .is_err()
    );
    // 新 refresh token（后 3 个）可以使用
    assert!(
        mgr.refresh(&tokens[3].refresh_token, &admin_profile(), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn same_device_replaces_old_session() {
    let mgr = make_manager();

    let pair1 = mgr.login(admin_login_params(1)).await.unwrap();
    let pair2 = mgr.login(admin_login_params(1)).await.unwrap();

    // 新 token 有效
    assert!(mgr.validate_token(&pair2.access_token).await.is_ok());

    // 只有一个设备在线
    let devices = mgr.get_devices(&LoginId::admin(1)).await.unwrap();
    assert_eq!(devices.len(), 1);

    // 旧 refresh token 不能使用（rid 已被删除）
    assert!(
        mgr.refresh(&pair1.refresh_token, &admin_profile(), None)
            .await
            .is_err()
    );
    // 新 refresh token 可以使用
    assert!(
        mgr.refresh(&pair2.refresh_token, &admin_profile(), None)
            .await
            .is_ok()
    );
}

// ── 不允许并发登录 ──

#[tokio::test]
async fn no_concurrent_login_clears_all_devices() {
    let mut config = default_config();
    config.concurrent_login = false;
    let mgr = SessionManager::new(Arc::new(MemoryStorage::new()), config);

    let mut p1 = admin_login_params(1);
    p1.device = DeviceType::Web;
    let pair1 = mgr.login(p1).await.unwrap();

    let mut p2 = admin_login_params(1);
    p2.device = DeviceType::Android;
    let _pair2 = mgr.login(p2).await.unwrap();

    // Web 的 refresh token 应该失效
    assert!(
        mgr.refresh(&pair1.refresh_token, &admin_profile(), None)
            .await
            .is_err()
    );

    // 只有 Android 设备在线
    let devices = mgr.get_devices(&LoginId::admin(1)).await.unwrap();
    assert_eq!(devices.len(), 1);
}

// ── 刷新 Token ──

#[tokio::test]
async fn refresh_token_works() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    let new_pair = mgr
        .refresh(&pair.refresh_token, &admin_profile(), None)
        .await
        .unwrap();

    // 移除 jti 后，同一秒内签发的 access token 是确定性的（相同 claims = 相同 token），
    // 所以不再断言 access_token 不同。刷新的核心是 refresh token 轮转。
    // refresh token 轮转
    assert_ne!(new_pair.refresh_token, pair.refresh_token);
    // 新 access token 有效
    assert!(mgr.validate_token(&new_pair.access_token).await.is_ok());
    // 旧 refresh token 不能再次使用（rid 已删除）
    assert!(
        mgr.refresh(&pair.refresh_token, &admin_profile(), None)
            .await
            .is_err()
    );
    // 新 refresh token 可以继续刷新
    let third_pair = mgr
        .refresh(&new_pair.refresh_token, &admin_profile(), None)
        .await
        .unwrap();
    assert_ne!(third_pair.refresh_token, new_pair.refresh_token);
    assert!(mgr.validate_token(&third_pair.access_token).await.is_ok());
}

#[tokio::test]
async fn refresh_with_updated_profile() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    // 刷新时传入更新后的 profile
    let updated_profile = UserProfile::Admin(AdminProfile {
        user_name: "test_user".to_string(),
        nick_name: "Updated Name".to_string(),
        roles: vec!["admin".to_string(), "editor".to_string()],
        permissions: vec![
            "system:user:list".to_string(),
            "system:user:add".to_string(),
            "system:user:delete".to_string(),
        ],
    });

    let new_pair = mgr
        .refresh(&pair.refresh_token, &updated_profile, None)
        .await
        .unwrap();

    // 新 token 包含更新后的信息
    let validated = mgr.validate_token(&new_pair.access_token).await.unwrap();
    assert_eq!(validated.nick_name, "Updated Name");
    assert_eq!(validated.roles, vec!["admin", "editor"]);
    assert!(
        validated
            .permissions
            .contains(&"system:user:delete".to_string())
    );
}

#[tokio::test]
async fn refresh_with_invalid_token_fails() {
    let mgr = make_manager();
    let result = mgr.refresh("invalid-refresh-token", &admin_profile(), None).await;
    assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
}

#[tokio::test]
async fn parse_refresh_token_returns_login_id() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = mgr.parse_refresh_token(&pair.refresh_token).unwrap();
    assert_eq!(login_id, LoginId::admin(1));
}

#[tokio::test]
async fn refresh_cross_type_rejected() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    // Access token 不能作为 refresh token
    let result = mgr.refresh(&pair.access_token, &admin_profile(), None).await;
    assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));

    // Refresh token 不能作为 access token
    let result = mgr.validate_token(&pair.refresh_token).await;
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

// ── Deny key 机制 ──

#[tokio::test]
async fn ban_user_blocks_access() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.ban_user(&LoginId::admin(1)).await.unwrap();

    // Access token → AccountBanned
    let result = mgr.validate_token(&pair.access_token).await;
    assert!(matches!(result, Err(AuthError::AccountBanned)));

    // Refresh 也失败（rid 被清理 or deny=banned）
    let result = mgr.refresh(&pair.refresh_token, &admin_profile(), None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn unban_user_restores_access() {
    let mgr = make_manager();

    mgr.ban_user(&LoginId::admin(1)).await.unwrap();
    mgr.unban_user(&LoginId::admin(1)).await.unwrap();

    // 重新登录应成功
    let pair = mgr.login(admin_login_params(1)).await.unwrap();
    assert!(mgr.validate_token(&pair.access_token).await.is_ok());
}

#[tokio::test]
async fn force_refresh_requires_token_refresh() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.force_refresh(&LoginId::admin(1)).await.unwrap();

    // Access token → RefreshRequired
    let result = mgr.validate_token(&pair.access_token).await;
    assert!(matches!(result, Err(AuthError::RefreshRequired)));

    // 等待 1 秒确保 refresh 生成的新 token 的 iat > deny_ts
    // （deny key 使用秒级时间戳，同一秒内签发的 token 仍被视为"旧"token）
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Refresh 仍然可以成功（deny="refresh:xxx" 不阻止 refresh 操作）
    let new_pair = mgr
        .refresh(&pair.refresh_token, &admin_profile(), None)
        .await
        .unwrap();
    assert!(mgr.validate_token(&new_pair.access_token).await.is_ok());
}

#[tokio::test]
async fn ban_during_refresh_blocked() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.ban_user(&LoginId::admin(1)).await.unwrap();

    // 封禁后 refresh 也应失败
    let result = mgr.refresh(&pair.refresh_token, &admin_profile(), None).await;
    assert!(result.is_err());
}

// ── 设备管理 ──

#[tokio::test]
async fn get_devices_returns_all() {
    let mgr = make_manager();

    let mut p1 = admin_login_params(1);
    p1.device = DeviceType::Web;
    p1.login_ip = "1.1.1.1".to_string();
    mgr.login(p1).await.unwrap();

    let mut p2 = admin_login_params(1);
    p2.device = DeviceType::Android;
    p2.login_ip = "2.2.2.2".to_string();
    mgr.login(p2).await.unwrap();

    let devices = mgr.get_devices(&LoginId::admin(1)).await.unwrap();
    assert_eq!(devices.len(), 2);
}

// ── 在线用户 ──

#[tokio::test]
async fn online_users_returns_total_and_items() {
    let mgr = make_manager();

    for i in 1..=3 {
        let mut p = admin_login_params(i);
        p.login_id = LoginId::admin(i);
        mgr.login(p).await.unwrap();
    }

    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: Some(UserType::Admin),
            page: 1,
            page_size: 2,
        })
        .await
        .unwrap();

    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 2);
}

#[tokio::test]
async fn online_users_page2() {
    let mgr = make_manager();
    for i in 1..=5 {
        let mut p = admin_login_params(i);
        p.login_id = LoginId::admin(i);
        mgr.login(p).await.unwrap();
    }

    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: None,
            page: 2,
            page_size: 2,
        })
        .await
        .unwrap();

    assert_eq!(page.total, 5);
    assert_eq!(page.items.len(), 2);
}

// ── 踢下线 ──

#[tokio::test]
async fn kick_out_single_device() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.kick_out(&LoginId::admin(1), Some(&DeviceType::Web))
        .await
        .unwrap();

    // deny="refresh" → RefreshRequired
    let result = mgr.validate_token(&pair.access_token).await;
    assert!(matches!(result, Err(AuthError::RefreshRequired)));
}

#[tokio::test]
async fn kick_out_all_devices() {
    let mgr = make_manager();

    let mut p1 = admin_login_params(1);
    p1.device = DeviceType::Web;
    let pair1 = mgr.login(p1).await.unwrap();

    let mut p2 = admin_login_params(1);
    p2.device = DeviceType::Android;
    let pair2 = mgr.login(p2).await.unwrap();

    mgr.kick_out(&LoginId::admin(1), None).await.unwrap();

    assert!(mgr.validate_token(&pair1.access_token).await.is_err());
    assert!(mgr.validate_token(&pair2.access_token).await.is_err());
}

// ── QR 码登录 ──

#[tokio::test]
async fn qr_code_full_flow() {
    let mgr = make_manager();

    let code = mgr.create_qr_code().await.unwrap();
    assert!(!code.is_empty());

    let state = mgr.get_qr_code_state(&code).await.unwrap();
    assert!(matches!(state, QrCodeState::Pending));

    let login_id = LoginId::admin(1);
    mgr.scan_qr_code(&code, &login_id).await.unwrap();

    let state = mgr.get_qr_code_state(&code).await.unwrap();
    assert!(matches!(state, QrCodeState::Scanned { .. }));

    mgr.confirm_qr_code(&code, admin_login_params(1))
        .await
        .unwrap();

    let state = mgr.get_qr_code_state(&code).await.unwrap();
    match state {
        QrCodeState::Confirmed { token_pair } => {
            assert!(!token_pair.access_token.is_empty());
            assert!(mgr.validate_token(&token_pair.access_token).await.is_ok());
        }
        _ => panic!("expected Confirmed state"),
    }
}

#[tokio::test]
async fn qr_code_cancel_flow() {
    let mgr = make_manager();

    let code = mgr.create_qr_code().await.unwrap();
    mgr.cancel_qr_code(&code).await.unwrap();

    let state = mgr.get_qr_code_state(&code).await.unwrap();
    assert!(matches!(state, QrCodeState::Cancelled));
}

#[tokio::test]
async fn qr_code_invalid_state_transitions() {
    let mgr = make_manager();
    let code = mgr.create_qr_code().await.unwrap();

    let result = mgr.confirm_qr_code(&code, admin_login_params(1)).await;
    assert!(matches!(result, Err(AuthError::QrCodeInvalidState)));

    let result = mgr.get_qr_code_state("non-existent").await;
    assert!(matches!(result, Err(AuthError::QrCodeNotFound)));
}

#[tokio::test]
async fn qr_code_scan_wrong_user_cannot_confirm() {
    let mgr = make_manager();
    let code = mgr.create_qr_code().await.unwrap();

    mgr.scan_qr_code(&code, &LoginId::admin(1)).await.unwrap();

    let mut params = admin_login_params(2);
    params.login_id = LoginId::admin(2);
    let result = mgr.confirm_qr_code(&code, params).await;
    assert!(matches!(result, Err(AuthError::QrCodeInvalidState)));
}

// ── MemoryStorage ──

use summer_auth::storage::AuthStorage;

use summer_auth::bitmap::PermissionMap;

#[tokio::test]
async fn memory_storage_purge_expired() {
    let storage = MemoryStorage::new();

    storage.set_string("test:expire", "value", 1).await.unwrap();
    assert!(storage.get_string("test:expire").await.unwrap().is_some());

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(storage.get_string("test:expire").await.unwrap().is_none());
}

#[tokio::test]
async fn memory_storage_keys_by_prefix() {
    let storage = MemoryStorage::new();

    storage
        .set_string("auth:device:admin:1:web", "d1", 3600)
        .await
        .unwrap();
    storage
        .set_string("auth:device:admin:1:android", "d2", 3600)
        .await
        .unwrap();
    storage
        .set_string("auth:device:biz:1:web", "d3", 3600)
        .await
        .unwrap();

    let keys = storage
        .keys_by_prefix("auth:device:admin:1:")
        .await
        .unwrap();
    assert_eq!(keys.len(), 2);

    let keys = storage.keys_by_prefix("auth:device:biz:").await.unwrap();
    assert_eq!(keys.len(), 1);
}

// ── 多用户类型 ──

#[tokio::test]
async fn business_user_login_and_validate() {
    let mgr = make_manager();
    let pair = mgr.login(biz_login_params(1)).await.unwrap();
    assert!(!pair.access_token.is_empty());

    let validated = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(validated.login_id, LoginId::business(1));
    assert_eq!(validated.nick_name, "Biz User");
    assert_eq!(validated.user_name, "biz_user");
    assert_eq!(validated.roles, vec!["merchant"]);
}

#[tokio::test]
async fn customer_user_login_and_validate() {
    let mgr = make_manager();
    let pair = mgr.login(customer_login_params(1)).await.unwrap();
    assert!(!pair.access_token.is_empty());

    let validated = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(validated.login_id, LoginId::customer(1));
    assert_eq!(validated.nick_name, "Customer");
    assert_eq!(validated.user_name, "");
    assert!(validated.roles.is_empty());
    assert!(validated.permissions.is_empty());
}

#[tokio::test]
async fn different_user_types_isolated() {
    let mgr = make_manager();

    let admin_pair = mgr.login(admin_login_params(1)).await.unwrap();
    let biz_pair = mgr.login(biz_login_params(1)).await.unwrap();
    let customer_pair = mgr.login(customer_login_params(1)).await.unwrap();

    assert!(mgr.validate_token(&admin_pair.access_token).await.is_ok());
    assert!(mgr.validate_token(&biz_pair.access_token).await.is_ok());
    assert!(
        mgr.validate_token(&customer_pair.access_token)
            .await
            .is_ok()
    );

    // 登出 admin 不影响 biz 和 customer
    mgr.logout_all(&LoginId::admin(1)).await.unwrap();
    assert!(mgr.validate_token(&admin_pair.access_token).await.is_err());
    assert!(mgr.validate_token(&biz_pair.access_token).await.is_ok());
    assert!(
        mgr.validate_token(&customer_pair.access_token)
            .await
            .is_ok()
    );
}

// ── 在线用户：混合类型 ──

#[tokio::test]
async fn online_users_mixed_types() {
    let mgr = make_manager();

    mgr.login(admin_login_params(1)).await.unwrap();
    mgr.login(biz_login_params(1)).await.unwrap();
    mgr.login(customer_login_params(1)).await.unwrap();

    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: None,
            page: 1,
            page_size: 10,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 3);

    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: Some(UserType::Business),
            page: 1,
            page_size: 10,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
}

// ── permission_matches 纯函数测试 ──

#[test]
fn permission_wildcard_trailing() {
    assert!(permission_matches("system:*", "system:user:list"));
    assert!(permission_matches("system:*", "system:role:add"));
    assert!(permission_matches("system:*", "system:menu:delete"));
    assert!(permission_matches("order:item:*", "order:item:list"));
    assert!(permission_matches("order:item:*", "order:item:edit"));
    assert!(!permission_matches("system:*", "order:list"));
    assert!(!permission_matches("order:item:*", "order:list"));
}

#[test]
fn permission_wildcard_super() {
    assert!(permission_matches("*", "system:user:list"));
    assert!(permission_matches("*", "any:thing:at:all"));
}

#[test]
fn permission_wildcard_middle() {
    assert!(permission_matches("system:*:list", "system:user:list"));
    assert!(permission_matches("system:*:list", "system:role:list"));
    assert!(!permission_matches("system:*:list", "system:user:add"));
}

#[test]
fn permission_exact_match() {
    assert!(permission_matches("system:user:list", "system:user:list"));
    assert!(!permission_matches("system:user:list", "system:user:add"));
}

#[test]
fn permission_segment_mismatch() {
    assert!(!permission_matches("system:user", "system:user:list"));
    assert!(!permission_matches(
        "system:user:list:detail",
        "system:user:list"
    ));
}

// ── 权限位图 (Permission Bitmap) ──

fn bitmap_perm_map() -> PermissionMap {
    PermissionMap::new(vec![
        ("system:user:list".to_string(), 0),
        ("system:user:add".to_string(), 1),
        ("system:role:list".to_string(), 2),
        ("system:role:add".to_string(), 3),
        ("order:list".to_string(), 4),
    ])
}

#[tokio::test]
async fn login_with_bitmap() {
    let mgr = make_manager();
    mgr.set_permission_map(bitmap_perm_map());

    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    // JWT 应该能验证通过
    let validated = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(validated.login_id, LoginId::admin(1));

    // 权限应该正确解码
    let mut perms = validated.permissions.clone();
    perms.sort();
    let mut expected = vec![
        "system:user:list".to_string(),
        "system:user:add".to_string(),
    ];
    expected.sort();
    assert_eq!(perms, expected);
}

#[tokio::test]
async fn validate_decodes_bitmap() {
    let mgr = make_manager();
    mgr.set_permission_map(bitmap_perm_map());

    let pair = mgr.login(admin_login_params(1)).await.unwrap();
    let validated = mgr.validate_token(&pair.access_token).await.unwrap();

    // 验证 permissions 被正确解码回字符串
    assert!(
        validated
            .permissions
            .contains(&"system:user:list".to_string())
    );
    assert!(
        validated
            .permissions
            .contains(&"system:user:add".to_string())
    );
    assert_eq!(validated.permissions.len(), 2);
}

#[tokio::test]
async fn no_perm_map_fallback() {
    let mgr = make_manager();
    // 不设置 PermissionMap

    let pair = mgr.login(admin_login_params(1)).await.unwrap();
    let validated = mgr.validate_token(&pair.access_token).await.unwrap();

    // 应该使用 permissions 数组回退
    assert_eq!(
        validated.permissions,
        vec!["system:user:list", "system:user:add"]
    );
}

#[tokio::test]
async fn refresh_with_bitmap() {
    let mgr = make_manager();
    mgr.set_permission_map(bitmap_perm_map());

    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    // 刷新时传入更新后的 profile
    let updated_profile = UserProfile::Admin(AdminProfile {
        user_name: "test_user".to_string(),
        nick_name: "Test User".to_string(),
        roles: vec!["admin".to_string()],
        permissions: vec![
            "system:user:list".to_string(),
            "system:user:add".to_string(),
            "system:role:list".to_string(),
        ],
    });

    let new_pair = mgr
        .refresh(&pair.refresh_token, &updated_profile, None)
        .await
        .unwrap();
    let validated = mgr.validate_token(&new_pair.access_token).await.unwrap();

    // 刷新后权限应包含新增的 role:list
    let mut perms = validated.permissions;
    perms.sort();
    assert_eq!(
        perms,
        vec!["system:role:list", "system:user:add", "system:user:list"]
    );
}

// ── Deny Key 多设备竞态修复验证 ──

#[tokio::test]
async fn deny_key_multi_device_race_condition_fixed() {
    let mgr = make_manager();

    // 两台设备登录同一用户
    let mut p1 = admin_login_params(1);
    p1.device = DeviceType::Web;
    let pair1 = mgr.login(p1).await.unwrap();

    let mut p2 = admin_login_params(1);
    p2.device = DeviceType::Android;
    let pair2 = mgr.login(p2).await.unwrap();

    // 管理员修改用户角色 → force_refresh
    mgr.force_refresh(&LoginId::admin(1)).await.unwrap();

    // 两台设备的旧 token 都应该返回 RefreshRequired
    assert!(matches!(
        mgr.validate_token(&pair1.access_token).await,
        Err(AuthError::RefreshRequired)
    ));
    assert!(matches!(
        mgr.validate_token(&pair2.access_token).await,
        Err(AuthError::RefreshRequired)
    ));

    // 等待 1 秒确保新 token 的 iat > deny_ts
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 设备 A 刷新 → 获得新 token
    let new_pair1 = mgr
        .refresh(&pair1.refresh_token, &admin_profile(), None)
        .await
        .unwrap();

    // 关键断言：设备 A 刷新后，新 token 应该通过验证
    assert!(mgr.validate_token(&new_pair1.access_token).await.is_ok());

    // 关键断言：设备 B 的旧 token 仍然被拒（deny key 未被删除）
    assert!(matches!(
        mgr.validate_token(&pair2.access_token).await,
        Err(AuthError::RefreshRequired)
    ));

    // 设备 B 也刷新 → 获得新 token
    let new_pair2 = mgr
        .refresh(&pair2.refresh_token, &admin_profile(), None)
        .await
        .unwrap();

    // 设备 B 的新 token 也应该通过验证
    assert!(mgr.validate_token(&new_pair2.access_token).await.is_ok());
}
