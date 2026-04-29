//! `GroupAuthStrategy` —— 按路由分组挂鉴权的统一抽象。
//!
//! 每个鉴权域（admin JWT / AI relay API Key / 未来的 MCP / webhook）实现一份
//! `GroupAuthStrategy`，由 [`crate::group_layer::GroupAuthLayer`] 包成 tower Layer，
//! 挂到对应的 group 路由上。策略自己决定：
//!
//! - token 从哪里取（Header / Cookie / query / body）
//! - 验证逻辑（JWT / sha256 查表 / HMAC / OAuth）
//! - 哪些路径豁免（`path_config` + 编译期 `#[no_auth]` 合并）
//! - 失败时给客户端返什么响应格式（admin 返 ProblemDetails；relay 要返 OpenAI 风格 error）
//!
//! 各 crate 的策略之间完全正交，切入一个新协议只需加一个 `impl`，不影响其他 group。

use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::response::Response;

use crate::path_auth::PathAuthConfig;

/// 组级鉴权策略。
///
/// `authenticate` 返 `Ok(())` 表示放行（可在其中往 `req.extensions_mut()` 注入
/// 域上下文，如 `UserSession` / `AiTokenContext`）；返 `Err(Response)` 则 Layer
/// 直接把这个 Response 作为最终响应返给客户端 —— 各域错误格式自行决定。
#[async_trait::async_trait]
pub trait GroupAuthStrategy: Send + Sync + 'static {
    /// 本策略管辖的 group 名（与 `TypedHandlerRegistrar::group()` 对齐）。
    ///
    /// 注册到 [`crate::group_layer::GroupAuthLayer`] 时决定把 middleware 挂到哪个组。
    fn group(&self) -> &'static str;

    /// include / exclude 规则。`None` 代表策略不依赖 `path_config`。
    ///
    /// 不由 Layer 前置调用——**strategy 自己**决定何时查 `requires_auth`。
    /// 譬如 JWT 的典型用法是"无 token 且路径需要鉴权 → 401；无 token 但路径豁免 → 放行"。
    fn path_config(&self) -> &PathAuthConfig;

    /// 执行鉴权。
    async fn authenticate(&self, req: &mut Request<Body>) -> Result<(), Response<Body>>;
}
