use std::sync::Arc;

use summer_auth::config::AuthConfig;
use summer_auth::error::AuthError;
use summer_auth::online::OnlineUserQuery;
use summer_auth::qrcode::QrCodeState;
use summer_auth::session::SessionManager;
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
            "qr_code_timeout": 300
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
        profile: UserProfile::Admin(AdminProfile {
            user_name: "test_user".to_string(),
            nick_name: "Test User".to_string(),
            avatar: String::new(),
            roles: vec!["admin".to_string()],
            permissions: vec!["system:user:list".to_string(), "system:user:add".to_string()],
        }),
    }
}

fn biz_login_params(user_id: i64) -> summer_auth::session::LoginParams {
    summer_auth::session::LoginParams {
        login_id: LoginId::business(user_id),
        device: DeviceType::Web,
        login_ip: "192.168.1.1".to_string(),
        user_agent: "biz-agent".to_string(),
        profile: UserProfile::Business(BusinessProfile {
            user_name: "biz_user".to_string(),
            nick_name: "Biz User".to_string(),
            avatar: String::new(),
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
        profile: UserProfile::Customer(CustomerProfile {
            nick_name: "Customer".to_string(),
            avatar: String::new(),
        }),
    }
}

// ── 登录/登出 ──

#[tokio::test]
async fn login_returns_token_pair() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    assert!(!pair.access_token.is_empty());
    assert!(!pair.refresh_token.is_empty());
    assert_eq!(pair.expires_in, 3600);
}

#[tokio::test]
async fn validate_token_after_login() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(login_id, LoginId::admin(1));
}

#[tokio::test]
async fn validate_invalid_token_fails() {
    let mgr = make_manager();
    let result = mgr.validate_token("non-existent-token").await;
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

#[tokio::test]
async fn logout_invalidates_token() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    mgr.logout(&LoginId::admin(1), &DeviceType::Web)
        .await
        .unwrap();

    let result = mgr.validate_token(&pair.access_token).await;
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

#[tokio::test]
async fn logout_all_invalidates_all_devices() {
    let mgr = make_manager();

    // 登录两个设备
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

    // session 中有两个设备
    let session = mgr.get_session(&LoginId::admin(1)).await.unwrap().unwrap();
    assert_eq!(session.devices.len(), 2);
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
    }

    // 第 4 个设备登录后，第 1 个（Web）应被踢掉
    assert!(mgr.validate_token(&tokens[0].access_token).await.is_err());
    // 后 3 个仍有效
    assert!(mgr.validate_token(&tokens[1].access_token).await.is_ok());
    assert!(mgr.validate_token(&tokens[2].access_token).await.is_ok());
    assert!(mgr.validate_token(&tokens[3].access_token).await.is_ok());

    let session = mgr.get_session(&LoginId::admin(1)).await.unwrap().unwrap();
    assert_eq!(session.devices.len(), 3);
}

#[tokio::test]
async fn same_device_replaces_old_token() {
    let mgr = make_manager();

    let pair1 = mgr.login(admin_login_params(1)).await.unwrap();
    let pair2 = mgr.login(admin_login_params(1)).await.unwrap();

    // 旧 token 失效
    assert!(mgr.validate_token(&pair1.access_token).await.is_err());
    // 新 token 有效
    assert!(mgr.validate_token(&pair2.access_token).await.is_ok());

    let session = mgr.get_session(&LoginId::admin(1)).await.unwrap().unwrap();
    assert_eq!(session.devices.len(), 1);
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

    // Web 设备的 token 应该失效
    assert!(mgr.validate_token(&pair1.access_token).await.is_err());

    let session = mgr.get_session(&LoginId::admin(1)).await.unwrap().unwrap();
    assert_eq!(session.devices.len(), 1);
    assert_eq!(session.devices[0].device, DeviceType::Android);
}

// ── share_token ──

#[tokio::test]
async fn share_token_reuses_existing() {
    let mut config = default_config();
    config.share_token = true;
    let mgr = SessionManager::new(Arc::new(MemoryStorage::new()), config);

    let pair1 = mgr.login(admin_login_params(1)).await.unwrap();
    let pair2 = mgr.login(admin_login_params(1)).await.unwrap();

    // 相同设备复用 token
    assert_eq!(pair1.access_token, pair2.access_token);
    assert_eq!(pair1.refresh_token, pair2.refresh_token);
}

// ── 刷新 Token ──

#[tokio::test]
async fn refresh_token_works() {
    let mgr = make_manager();
    let pair = mgr.login(admin_login_params(1)).await.unwrap();

    let new_pair = mgr.refresh(&pair.refresh_token).await.unwrap();

    // 新 access token 不同
    assert_ne!(new_pair.access_token, pair.access_token);
    // refresh token 保持不变
    assert_eq!(new_pair.refresh_token, pair.refresh_token);
    // 新 access token 有效
    assert!(mgr.validate_token(&new_pair.access_token).await.is_ok());
    // 旧 access token 失效
    assert!(mgr.validate_token(&pair.access_token).await.is_err());
}

#[tokio::test]
async fn refresh_with_invalid_token_fails() {
    let mgr = make_manager();
    let result = mgr.refresh("invalid-refresh-token").await;
    assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
}

// ── RBAC ──

#[tokio::test]
async fn login_sets_roles_and_permissions() {
    let mgr = make_manager();
    mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = LoginId::admin(1);
    assert!(mgr.has_role(&login_id, "admin").await.unwrap());
    assert!(!mgr.has_role(&login_id, "user").await.unwrap());

    assert!(mgr.has_permission(&login_id, "system:user:list").await.unwrap());
    assert!(!mgr.has_permission(&login_id, "system:user:delete").await.unwrap());
}

#[tokio::test]
async fn check_role_returns_error_on_missing() {
    let mgr = make_manager();
    mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = LoginId::admin(1);
    assert!(mgr.check_role(&login_id, "admin").await.is_ok());
    assert!(matches!(
        mgr.check_role(&login_id, "superadmin").await,
        Err(AuthError::NoRole(_))
    ));
}

#[tokio::test]
async fn check_permission_returns_error_on_missing() {
    let mgr = make_manager();
    mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = LoginId::admin(1);
    assert!(mgr.check_permission(&login_id, "system:user:list").await.is_ok());
    assert!(matches!(
        mgr.check_permission(&login_id, "system:user:delete").await,
        Err(AuthError::NoPermission(_))
    ));
}

#[tokio::test]
async fn set_roles_updates_session() {
    let mgr = make_manager();
    mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = LoginId::admin(1);
    mgr.set_roles(&login_id, vec!["editor".to_string()])
        .await
        .unwrap();

    assert!(!mgr.has_role(&login_id, "admin").await.unwrap());
    assert!(mgr.has_role(&login_id, "editor").await.unwrap());
}

#[tokio::test]
async fn set_permissions_updates_session() {
    let mgr = make_manager();
    mgr.login(admin_login_params(1)).await.unwrap();

    let login_id = LoginId::admin(1);
    mgr.set_permissions(&login_id, vec!["new:perm".to_string()])
        .await
        .unwrap();

    assert!(!mgr.has_permission(&login_id, "system:user:list").await.unwrap());
    assert!(mgr.has_permission(&login_id, "new:perm").await.unwrap());
}

// ── 在线用户 ──

#[tokio::test]
async fn online_users_returns_total_and_items() {
    let mgr = make_manager();

    // 登录 3 个不同用户
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
    assert_eq!(page.items.len(), 2); // 分页限制
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

    assert!(mgr.validate_token(&pair.access_token).await.is_err());
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

    // 1. Web 端创建 QR 码
    let code = mgr.create_qr_code().await.unwrap();
    assert!(!code.is_empty());

    // 2. 查询状态 → Pending
    let state = mgr.get_qr_code_state(&code).await.unwrap();
    assert!(matches!(state, QrCodeState::Pending));

    // 3. 移动端扫码
    let login_id = LoginId::admin(1);
    mgr.scan_qr_code(&code, &login_id).await.unwrap();

    let state = mgr.get_qr_code_state(&code).await.unwrap();
    assert!(matches!(state, QrCodeState::Scanned { .. }));

    // 4. 移动端确认
    mgr.confirm_qr_code(&code, admin_login_params(1)).await.unwrap();

    // 5. Web 端拿到 token
    let state = mgr.get_qr_code_state(&code).await.unwrap();
    match state {
        QrCodeState::Confirmed { token_pair } => {
            assert!(!token_pair.access_token.is_empty());
            // token 应该能验证通过
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

    // 不能在 Pending 状态直接 confirm
    let result = mgr.confirm_qr_code(&code, admin_login_params(1)).await;
    assert!(matches!(result, Err(AuthError::QrCodeInvalidState)));

    // 不能对不存在的 code 操作
    let result = mgr.get_qr_code_state("non-existent").await;
    assert!(matches!(result, Err(AuthError::QrCodeNotFound)));
}

#[tokio::test]
async fn qr_code_scan_wrong_user_cannot_confirm() {
    let mgr = make_manager();
    let code = mgr.create_qr_code().await.unwrap();

    // 用户 1 扫码
    mgr.scan_qr_code(&code, &LoginId::admin(1)).await.unwrap();

    // 用户 2 尝试确认 → 应该失败（login_id 不匹配）
    let mut params = admin_login_params(2);
    params.login_id = LoginId::admin(2);
    let result = mgr.confirm_qr_code(&code, params).await;
    assert!(matches!(result, Err(AuthError::QrCodeInvalidState)));
}

// ── MemoryStorage 过期清理 ──

#[tokio::test]
async fn memory_storage_purge_expired() {
    let storage = MemoryStorage::new();

    // 存一个 TTL=1 的值
    storage
        .set_string("test:expire", "value", 1)
        .await
        .unwrap();

    // 立即读取应该存在
    assert!(storage.get_string("test:expire").await.unwrap().is_some());

    // 等待过期
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 过期后读取返回 None
    assert!(storage.get_string("test:expire").await.unwrap().is_none());
}

#[tokio::test]
async fn memory_storage_keys_by_prefix() {
    let storage = MemoryStorage::new();

    storage
        .set_string("auth:admin:session:1", "s1", 3600)
        .await
        .unwrap();
    storage
        .set_string("auth:admin:session:2", "s2", 3600)
        .await
        .unwrap();
    storage
        .set_string("auth:biz:session:1", "s3", 3600)
        .await
        .unwrap();

    let keys = storage
        .keys_by_prefix("auth:admin:session:")
        .await
        .unwrap();
    assert_eq!(keys.len(), 2);

    let keys = storage.keys_by_prefix("auth:biz:").await.unwrap();
    assert_eq!(keys.len(), 1);
}

// 需要引入 AuthStorage trait
use summer_auth::storage::AuthStorage;

// ── 多用户类型测试 ──

#[tokio::test]
async fn business_user_login_and_session() {
    let mgr = make_manager();
    let pair = mgr.login(biz_login_params(1)).await.unwrap();

    assert!(!pair.access_token.is_empty());

    let login_id = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(login_id, LoginId::business(1));

    let session = mgr.get_session(&LoginId::business(1)).await.unwrap().unwrap();
    assert_eq!(session.profile.nick_name(), "Biz User");
    assert_eq!(session.profile.user_name(), "biz_user");
    assert_eq!(session.profile.roles(), &["merchant".to_string()]);
}

#[tokio::test]
async fn customer_user_login_and_session() {
    let mgr = make_manager();
    let pair = mgr.login(customer_login_params(1)).await.unwrap();

    assert!(!pair.access_token.is_empty());

    let login_id = mgr.validate_token(&pair.access_token).await.unwrap();
    assert_eq!(login_id, LoginId::customer(1));

    let session = mgr.get_session(&LoginId::customer(1)).await.unwrap().unwrap();
    assert_eq!(session.profile.nick_name(), "Customer");
    assert_eq!(session.profile.user_name(), ""); // Customer 无 user_name
    assert!(session.profile.roles().is_empty()); // Customer 无 RBAC
    assert!(session.profile.permissions().is_empty());
}

#[tokio::test]
async fn different_user_types_isolated() {
    let mgr = make_manager();

    // 同一 user_id 不同类型完全隔离
    let admin_pair = mgr.login(admin_login_params(1)).await.unwrap();
    let biz_pair = mgr.login(biz_login_params(1)).await.unwrap();
    let customer_pair = mgr.login(customer_login_params(1)).await.unwrap();

    // 各自 token 都有效
    assert!(mgr.validate_token(&admin_pair.access_token).await.is_ok());
    assert!(mgr.validate_token(&biz_pair.access_token).await.is_ok());
    assert!(mgr.validate_token(&customer_pair.access_token).await.is_ok());

    // 登出 admin 不影响 biz 和 customer
    mgr.logout_all(&LoginId::admin(1)).await.unwrap();
    assert!(mgr.validate_token(&admin_pair.access_token).await.is_err());
    assert!(mgr.validate_token(&biz_pair.access_token).await.is_ok());
    assert!(mgr.validate_token(&customer_pair.access_token).await.is_ok());
}

#[tokio::test]
async fn customer_set_roles_is_noop() {
    let mgr = make_manager();
    mgr.login(customer_login_params(1)).await.unwrap();

    let login_id = LoginId::customer(1);
    // set_roles 对 Customer 是无操作
    mgr.set_roles(&login_id, vec!["some_role".to_string()])
        .await
        .unwrap();

    // 仍然没有角色
    assert!(!mgr.has_role(&login_id, "some_role").await.unwrap());
}

#[tokio::test]
async fn online_users_mixed_types() {
    let mgr = make_manager();

    mgr.login(admin_login_params(1)).await.unwrap();
    mgr.login(biz_login_params(1)).await.unwrap();
    mgr.login(customer_login_params(1)).await.unwrap();

    // 查询所有类型
    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: None,
            page: 1,
            page_size: 10,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 3);

    // 仅查询 Business
    let page = mgr
        .online_users(OnlineUserQuery {
            user_type: Some(UserType::Business),
            page: 1,
            page_size: 10,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].nick_name, "Biz User");
}

// ── JWT 模式测试 ──

mod jwt_tests {
    use std::sync::Arc;

    use summer_auth::config::AuthConfig;
    use summer_auth::error::AuthError;
    use summer_auth::session::SessionManager;
    use summer_auth::storage::memory::MemoryStorage;
    use summer_auth::user_type::{DeviceType, LoginId};
    use summer_auth::{AdminProfile, UserProfile};

    fn jwt_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "concurrent_login": true,
                "max_devices": 3,
                "qr_code_timeout": 300,
                "token_style": "jwt",
                "jwt_secret": "test-jwt-secret-key-for-integration-tests-32chars!"
            }"#,
        )
        .unwrap()
    }

    fn make_jwt_manager() -> SessionManager {
        let storage = Arc::new(MemoryStorage::new());
        SessionManager::new(storage, jwt_config())
    }

    fn admin_login_params(user_id: i64) -> summer_auth::session::LoginParams {
        summer_auth::session::LoginParams {
            login_id: LoginId::admin(user_id),
            device: DeviceType::Web,
            login_ip: "127.0.0.1".to_string(),
            user_agent: "test-agent".to_string(),
            profile: UserProfile::Admin(AdminProfile {
                user_name: "test_user".to_string(),
                nick_name: "Test User".to_string(),
                avatar: String::new(),
                roles: vec!["admin".to_string()],
                permissions: vec!["system:user:list".to_string()],
            }),
        }
    }

    #[tokio::test]
    async fn jwt_login_returns_jwt_tokens() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        // JWT 格式：3 段 base64 用 . 分隔
        assert_eq!(pair.access_token.split('.').count(), 3);
        assert_eq!(pair.refresh_token.split('.').count(), 3);
        assert_eq!(pair.expires_in, 3600);
    }

    #[tokio::test]
    async fn jwt_validate_token_works() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        let login_id = mgr.validate_token(&pair.access_token).await.unwrap();
        assert_eq!(login_id, LoginId::admin(1));
    }

    #[tokio::test]
    async fn jwt_logout_blacklists_token() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        // 验证 token 有效
        assert!(mgr.validate_token(&pair.access_token).await.is_ok());

        // 登出
        mgr.logout(&LoginId::admin(1), &DeviceType::Web)
            .await
            .unwrap();

        // token 被黑名单拦截
        let result = mgr.validate_token(&pair.access_token).await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[tokio::test]
    async fn jwt_refresh_works() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        let new_pair = mgr.refresh(&pair.refresh_token).await.unwrap();

        // 新 access token 不同
        assert_ne!(new_pair.access_token, pair.access_token);
        // refresh token 保持不变
        assert_eq!(new_pair.refresh_token, pair.refresh_token);
        // 新 access token 有效
        assert!(mgr.validate_token(&new_pair.access_token).await.is_ok());
        // 旧 access token 失效（被黑名单）
        assert!(mgr.validate_token(&pair.access_token).await.is_err());
    }

    #[tokio::test]
    async fn jwt_invalid_token_rejected() {
        let mgr = make_jwt_manager();

        let result = mgr.validate_token("not.a.valid-jwt").await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));

        let result = mgr.validate_token("completely-invalid").await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[tokio::test]
    async fn jwt_multi_device_and_max_devices() {
        let mgr = make_jwt_manager(); // max_devices = 3

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
        }

        // 第 4 个设备登录后，第 1 个（Web）应被踢掉（黑名单方式）
        assert!(mgr.validate_token(&tokens[0].access_token).await.is_err());
        // 后 3 个仍有效
        assert!(mgr.validate_token(&tokens[1].access_token).await.is_ok());
        assert!(mgr.validate_token(&tokens[2].access_token).await.is_ok());
        assert!(mgr.validate_token(&tokens[3].access_token).await.is_ok());

        let session = mgr.get_session(&LoginId::admin(1)).await.unwrap().unwrap();
        assert_eq!(session.devices.len(), 3);
    }

    #[tokio::test]
    async fn jwt_opaque_coexist_unchanged() {
        // jwt feature 启用时，opaque 模式（uuid）仍正常工作
        let config: AuthConfig = serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "concurrent_login": true,
                "max_devices": 3,
                "token_style": "uuid"
            }"#,
        )
        .unwrap();

        let mgr = SessionManager::new(Arc::new(MemoryStorage::new()), config);
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        // UUID 格式（36 字符，含连字符）
        assert_eq!(pair.access_token.len(), 36);
        assert!(pair.access_token.contains('-'));

        // 验证 + 登出正常
        let login_id = mgr.validate_token(&pair.access_token).await.unwrap();
        assert_eq!(login_id, LoginId::admin(1));

        mgr.logout(&LoginId::admin(1), &DeviceType::Web)
            .await
            .unwrap();
        assert!(mgr.validate_token(&pair.access_token).await.is_err());
    }

    #[tokio::test]
    async fn jwt_refresh_with_access_token_fails() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        // 用 access_token 作为 refresh_token 应被拒绝（类型不匹配）
        let result = mgr.refresh(&pair.access_token).await;
        assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
    }

    #[tokio::test]
    async fn jwt_validate_with_refresh_token_fails() {
        let mgr = make_jwt_manager();
        let pair = mgr.login(admin_login_params(1)).await.unwrap();

        // 用 refresh_token 作为 access_token 应被拒绝（类型不匹配）
        let result = mgr.validate_token(&pair.refresh_token).await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }
}
