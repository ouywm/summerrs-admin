pub mod job_router;

use summer_auth::{GroupAuthLayer, JwtStrategy};
use summer_web::Router;

pub fn router() -> Router {
    summer_web::handler::grouped_router(crate::job_dynamic_group())
}

pub fn router_with_layers() -> Router {
    router().layer(GroupAuthLayer::new(JwtStrategy::for_group(
        crate::job_dynamic_group(),
    )))
}
