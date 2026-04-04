use super::*;

async fn default_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string())
}

async fn default_redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| DEFAULT_REDIS_URL.to_string())
}

pub(super) async fn shared_test_db() -> summer_sea_orm::DbConn {
    Database::connect(default_database_url().await)
        .await
        .expect("connect test db")
}

pub(super) async fn shared_test_redis() -> summer_redis::Redis {
    summer_redis::redis::Client::open(default_redis_url().await)
        .expect("create redis client")
        .get_connection_manager()
        .await
        .expect("connect redis")
}

pub(crate) async fn response_json(response: Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&body).expect("json response body")
}

pub(crate) async fn response_text(response: Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    String::from_utf8(body.to_vec()).expect("utf8 response body")
}

pub(super) async fn build_test_router(
    db: summer_sea_orm::DbConn,
    redis: summer_redis::Redis,
) -> Router {
    let mut app = App::new();
    app.add_component(db.clone());
    app.add_component(redis);
    app.add_component(UpstreamHttpClient::build().expect("build upstream http client"));
    app.add_component(AiLogBatchQueue::immediate(db));

    let app = app.build().await.expect("build test app");
    auto_router()
        .route_layer(AiAuthLayer::new())
        .layer(ClientIpSource::RightmostXForwardedFor.into_extension())
        .layer(Extension(AppState { app }))
}
