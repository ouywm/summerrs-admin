use std::time::Instant;
use summer_admin_macros::rate_limit;
use summer_auth::{DeviceType, LoginId, UserProfile, UserSession};
use summer_common::error::ApiResult;
use summer_common::rate_limit::middleware::rate_limit_headers_middleware;
use summer_common::rate_limit::{RateLimitEngine, RateLimitEngineConfig};
use summer_web::axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
    middleware,
};
use summer_web::handler::TypeRouter;
use summer_web::{Router, get_api};
use tower::util::ServiceExt;

#[rate_limit(rate = 2, per = "second", key = "ip")]
#[get_api("/limited")]
async fn limited_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(rate = 1, per = "second", key = "user")]
#[get_api("/user-limited")]
async fn user_limited_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(rate = 2, per = "second", key = "ip", algorithm = "sliding_window")]
#[get_api("/sliding-window")]
async fn sliding_window_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(rate = 1, per = "second", key = "ip", algorithm = "leaky_bucket")]
#[get_api("/leaky-bucket")]
async fn leaky_bucket_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(
    rate = 1,
    per = "second",
    key = "ip",
    algorithm = "throttle_queue",
    max_wait_ms = 1500
)]
#[get_api("/throttle-queue")]
async fn throttle_queue_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(
    rate = 1,
    per = "second",
    key = "ip",
    algorithm = "throttle_queue",
    max_wait_ms = 500
)]
#[get_api("/throttle-queue-short")]
async fn throttle_queue_short_handler() -> ApiResult<()> {
    Ok(())
}

#[rate_limit(rate = 1, per = "second", burst = 3, algorithm = "gcra", key = "ip")]
#[get_api("/gcra")]
async fn gcra_handler() -> ApiResult<()> {
    Ok(())
}

// ---- 新增：Shadow Mode ----
#[rate_limit(rate = 1, per = "second", key = "ip", mode = "shadow")]
#[get_api("/shadow")]
async fn shadow_handler() -> ApiResult<()> {
    Ok(())
}

fn admin_session(user_id: i64) -> UserSession {
    UserSession {
        login_id: LoginId::new(user_id),
        device: DeviceType::Web,
        profile: UserProfile {
            user_name: format!("admin-{user_id}"),
            nick_name: format!("Admin {user_id}"),
            roles: vec!["admin".to_string()],
            permissions: vec!["*:*:*".to_string()],
        },
    }
}

async fn test_router() -> Router {
    Router::new()
        .typed_route(limited_handler)
        .typed_route(user_limited_handler)
        .typed_route(sliding_window_handler)
        .typed_route(leaky_bucket_handler)
        .typed_route(throttle_queue_handler)
        .typed_route(throttle_queue_short_handler)
        .typed_route(gcra_handler)
        .typed_route(shadow_handler)
        .layer(Extension(RateLimitEngine::new(None)))
}

/// 带 HTTP headers middleware 的 router（用于响应头测试）。
async fn test_router_with_headers() -> Router {
    Router::new()
        .typed_route(limited_handler)
        .typed_route(shadow_handler)
        .layer(middleware::from_fn(rate_limit_headers_middleware))
        .layer(Extension(RateLimitEngine::new(None)))
}

#[tokio::test]
async fn third_request_returns_429() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/limited")
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("second response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router.oneshot(request()).await.expect("third response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn user_key_isolated_by_user_id() {
    let router = test_router().await;

    let request = |user_id| {
        Request::builder()
            .method(Method::GET)
            .uri("/user-limited")
            .extension(admin_session(user_id))
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request(1))
        .await
        .expect("user 1 first response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(request(1))
        .await
        .expect("user 1 second response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let response = router.oneshot(request(2)).await.expect("user 2 response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sliding_window_third_request_returns_429() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/sliding-window")
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("second response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router.oneshot(request()).await.expect("third response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn leaky_bucket_rejects_until_interval_passes() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/leaky-bucket")
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("second response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let response = router.oneshot(request()).await.expect("third response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn throttle_queue_waits_before_second_request() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/throttle-queue")
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(response.status(), StatusCode::OK);

    let started_at = Instant::now();
    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("second response");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(started_at.elapsed().as_millis() >= 900);
}

#[tokio::test]
async fn throttle_queue_rejects_when_wait_exceeds_limit() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/throttle-queue-short")
            .body(Body::empty())
            .expect("build request")
    };

    let response = router
        .clone()
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router.oneshot(request()).await.expect("second response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// 验证 `algorithm = "gcra"` + `burst` 联动：rate=1/秒、burst=3 时
/// 突发允许 3 个请求，第 4 个被拒绝。
#[tokio::test]
async fn gcra_with_burst_allows_three_then_rejects_fourth() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/gcra")
            .body(Body::empty())
            .expect("build request")
    };

    for _ in 0..3 {
        let response = router
            .clone()
            .oneshot(request())
            .await
            .expect("burst response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = router.oneshot(request()).await.expect("denied response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---- 新增：Shadow Mode 集成 ----

/// Shadow 模式下命中限流不会真正拒绝（仍 200）。
#[tokio::test]
async fn shadow_mode_does_not_reject() {
    let router = test_router().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/shadow")
            .body(Body::empty())
            .expect("build request")
    };

    // rate = 1，但 shadow 模式下连续 5 个请求都应该是 200
    for _ in 0..5 {
        let response = router
            .clone()
            .oneshot(request())
            .await
            .expect("shadow response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}

// ---- 新增：HTTP RateLimit-* 响应头 ----

#[tokio::test]
async fn rate_limit_headers_are_injected_on_success() {
    let router = test_router_with_headers().await;

    let response = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/limited")
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get("ratelimit-limit").is_some(),
        "expected RateLimit-Limit header"
    );
    assert!(
        response.headers().get("ratelimit-remaining").is_some(),
        "expected RateLimit-Remaining header"
    );
    assert!(
        response.headers().get("ratelimit-reset").is_some(),
        "expected RateLimit-Reset header"
    );
}

#[tokio::test]
async fn rate_limit_headers_include_retry_after_on_rejection() {
    let router = test_router_with_headers().await;

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/limited")
            .body(Body::empty())
            .expect("build request")
    };

    // 把 rate 用完（limited rate=2）
    router.clone().oneshot(request()).await.unwrap();
    router.clone().oneshot(request()).await.unwrap();

    let response = router.oneshot(request()).await.expect("denied response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response.headers().get("retry-after").is_some(),
        "expected Retry-After header on 429"
    );
}

// ---- 新增：Allowlist / Blocklist via engine config ----

#[tokio::test]
async fn allowlist_passes_unconditionally() {
    // 测试用 127.0.0.1（axum-client-ip 默认 fallback 的 IP）作为 allowlist
    let engine = RateLimitEngine::with_config(
        None,
        RateLimitEngineConfig {
            allowlist: vec!["127.0.0.1/32".parse().unwrap()],
            ..Default::default()
        },
    );

    let router = Router::new()
        .typed_route(limited_handler)
        .layer(Extension(engine));

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/limited")
            .body(Body::empty())
            .expect("build request")
    };

    // limited rate=2，但 allowlist 命中（127.0.0.1），连续 100 次都应该 200
    for _ in 0..100 {
        let response = router.clone().oneshot(request()).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}
