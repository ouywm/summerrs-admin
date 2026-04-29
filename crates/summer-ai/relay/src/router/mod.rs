//! summer-ai-relay HTTP 路由模块

use summer_web::Router;

pub mod claude;
pub mod gemini;
pub mod openai;

pub fn router() -> Router {
    summer_web::handler::grouped_router(crate::relay_group())
}
