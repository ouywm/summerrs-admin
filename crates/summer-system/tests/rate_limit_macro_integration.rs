use std::time::Instant;
use summer_admin_macros::rate_limit;
use summer_auth::{AdminProfile, DeviceType, LoginId, UserProfile, UserSession};
use summer_common::error::ApiResult;
use summer_common::rate_limit::RateLimitEngine;
use summer_web::axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
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

fn admin_session(user_id: i64) -> UserSession {
    UserSession {
        login_id: LoginId::admin(user_id),
        device: DeviceType::Web,
        tenant_id: None,
        profile: UserProfile::Admin(AdminProfile {
            user_name: format!("admin-{user_id}"),
            nick_name: format!("Admin {user_id}"),
            roles: vec!["admin".to_string()],
            permissions: vec!["*:*:*".to_string()],
        }),
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
