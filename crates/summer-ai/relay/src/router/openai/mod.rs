//! OpenAI 协议入口（`/v1/*` 端点）。

pub mod chat;
pub mod models;
pub mod responses;

use summer_web::Router;

pub fn routes(router: Router) -> Router {
    let router = chat::routes(router);
    let router = responses::routes(router);
    models::routes(router)
}
