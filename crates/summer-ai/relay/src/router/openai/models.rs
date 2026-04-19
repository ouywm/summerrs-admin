//! `GET /v1/models` —— OpenAI 兼容模型列表接口。
//!
//! **当前**：走路骨架阶段，直接向 OpenAI 官方拉 `/v1/models`。
//! **后续**：P4 从 `ai.channel.models` JSONB 聚合去重。

use summer_admin_macros::no_auth;
use summer_ai_core::ModelList;
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::http::StatusCode;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::get;
use summer_web::handler::TypeRouter;

const UPSTREAM_MODELS_URL: &str = "https://wzw.pp.ua/v1/models";

/// `GET /v1/models`
#[no_auth]
#[get("/v1/models")]
pub async fn list_models(Component(http): Component<reqwest::Client>) -> Response {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {
                        "message": "OPENAI_API_KEY environment variable not set",
                        "type": "configuration_error"
                    }
                })),
            )
                .into_response();
        }
    };

    match http
        .get(UPSTREAM_MODELS_URL)
        .bearer_auth(&api_key)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<ModelList>().await {
            Ok(models) => Json(models).into_response(),
            Err(error) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {"message": error.to_string(), "type": "parse_error"}
                })),
            )
                .into_response(),
        },
        Ok(resp) => {
            let status =
                StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let body = resp.bytes().await.unwrap_or_default();
            (status, body).into_response()
        }
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {"message": error.to_string(), "type": "upstream_unreachable"}
            })),
        )
            .into_response(),
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(list_models)
}
