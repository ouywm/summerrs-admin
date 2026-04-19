//! Claude Messages API 入口路由（`POST /v1/messages`）。

pub mod messages;

use summer_web::Router;

pub fn routes(router: Router) -> Router {
    messages::routes(router)
}
