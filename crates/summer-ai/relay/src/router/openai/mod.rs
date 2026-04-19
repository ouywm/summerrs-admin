//! OpenAI 协议入口（`/v1/*` 端点）。

pub mod chat;
pub mod models;

use summer_web::Router;

pub fn routes(router: Router) -> Router {
    let router = chat::routes(router);
    models::routes(router)
}
