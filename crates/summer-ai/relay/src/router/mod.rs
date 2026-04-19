//! summer-ai-relay HTTP 路由组装。
//!
//! 按**入口协议**分子目录（多入口协议）：
//!
//! - `openai/` — `/v1/chat/completions` / `/v1/models` / `/v1/embeddings` / ...
//! - `claude/` — `/v1/messages`
//! - `gemini/` — `/v1beta/models/*/generateContent`

pub mod claude;
pub mod openai;

use summer_web::Router;

/// 组装 relay 暴露的全部 HTTP 路由。
pub fn relay_router() -> Router {
    let router = Router::new();
    let router = openai::routes(router);
    claude::routes(router)
}
