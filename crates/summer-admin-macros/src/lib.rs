mod auth_macro;
mod log_macro;
mod rate_limit_macro;

use proc_macro::TokenStream;

/// 操作日志属性宏
///
/// 自动记录接口的操作日志，包括请求参数、响应内容、耗时、操作人等信息。
///
/// # 参数
///
/// - `module`：业务模块名称（必填）
/// - `action`：操作描述（必填）
/// - `biz_type`：操作类型（必填），可选值：Other / Create / Update / Delete / Query / Export / Import / Auth
/// - `save_params`：是否记录请求参数（可选，默认 true，敏感接口设为 false）
/// - `save_response`：是否记录响应内容（可选，默认 true）
///
/// # 示例
///
/// ```rust,ignore
/// #[log(module = "用户管理", action = "创建用户", biz_type = Create)]
/// #[post("/user")]
/// pub async fn create_user(...) -> ApiResult<()> { ... }
///
/// #[log(module = "认证管理", action = "用户登录", biz_type = Other, save_params = false)]
/// #[post("/auth/login")]
/// pub async fn login(...) -> ApiResult<Json<LoginVo>> { ... }
/// ```
#[proc_macro_attribute]
pub fn log(args: TokenStream, input: TokenStream) -> TokenStream {
    log_macro::expand(args, input)
}

/// 登录校验属性宏
///
/// 注入 `LoginUser` 提取器确保请求方已登录，未登录时返回 401。
///
/// # 示例
///
/// ```rust,ignore
/// #[login]
/// #[get("/profile")]
/// pub async fn get_profile() -> ApiResult<Json<Profile>> { ... }
/// ```
#[proc_macro_attribute]
pub fn login(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_check_login(args, input)
}

/// 公共接口（免鉴权）属性宏
///
/// 将该 handler 标记为“无需携带 token 也可访问”，用于配合 `summer-auth` 中间件的
/// PathAuthConfig.exclude 规则。
///
/// 支持两种模式：
/// - 自动：`#[public]`（从 `#[get_api("/x")]` / `#[post_api("/y")]` 等路由宏推导 method+path）
/// - 手动：`#[public(GET, "/x")]` / `#[public("/x")]`
#[proc_macro_attribute]
pub fn public(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_public_route(args, input)
}

/// `#[no_auth]` 等价于 `#[public]`
#[proc_macro_attribute]
pub fn no_auth(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_public_route(args, input)
}

/// 单权限校验属性宏（支持通配符匹配）
///
/// 注入登录校验 + 权限检查。权限不足时返回 403。
/// 支持 `*` 通配符：`system:*` 匹配 `system:user:list` 等。
///
/// # 示例
///
/// ```rust,ignore
/// #[has_perm("system:user:list")]
/// #[get("/user/list")]
/// pub async fn list_users(...) -> ApiResult<Json<...>> { ... }
/// ```
#[proc_macro_attribute]
pub fn has_perm(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_check_permission(args, input)
}

/// 单角色校验属性宏
///
/// 注入登录校验 + 角色检查。角色不足时返回 403。
///
/// # 示例
///
/// ```rust,ignore
/// #[has_role("admin")]
/// #[get("/admin/dashboard")]
/// pub async fn dashboard(...) -> ApiResult<Json<...>> { ... }
/// ```
#[proc_macro_attribute]
pub fn has_role(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_check_role(args, input)
}

/// 多权限校验属性宏（支持 AND/OR 逻辑 + 通配符）
///
/// - `and(...)` — 必须拥有全部权限
/// - `or(...)` — 拥有任一权限即可
///
/// # 示例
///
/// ```rust,ignore
/// #[has_perms(and("system:user:list", "system:user:add"))]
/// #[post("/user")]
/// pub async fn create_user(...) -> ApiResult<()> { ... }
///
/// #[has_perms(or("system:user:list", "system:role:list"))]
/// #[get("/overview")]
/// pub async fn overview(...) -> ApiResult<Json<...>> { ... }
/// ```
#[proc_macro_attribute]
pub fn has_perms(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_check_permissions(args, input)
}

/// 多角色校验属性宏（支持 AND/OR 逻辑）
///
/// - `and(...)` — 必须拥有全部角色
/// - `or(...)` — 拥有任一角色即可
///
/// # 示例
///
/// ```rust,ignore
/// #[has_roles(and("admin", "editor"))]
/// #[put("/content")]
/// pub async fn edit_content(...) -> ApiResult<()> { ... }
///
/// #[has_roles(or("admin", "moderator"))]
/// #[delete("/post/{id}")]
/// pub async fn delete_post(...) -> ApiResult<()> { ... }
/// ```
#[proc_macro_attribute]
pub fn has_roles(args: TokenStream, input: TokenStream) -> TokenStream {
    auth_macro::expand_check_roles(args, input)
}

/// `#[rate_limit]` - 声明式限流（对 HTTP handler 生效）
///
/// 该宏会：
///
/// - 为 handler **自动注入** `summer_common::rate_limit::RateLimitContext` extractor
/// - 在业务逻辑执行前调用 `RateLimitContext::check(...)` 进行限流
///
/// # 前置条件
///
/// 需要在应用里提供 `summer_common::rate_limit::RateLimitEngine`（二选一）：
///
/// 1. 作为 axum layer 注入：`router.layer(Extension(RateLimitEngine::new(...)))`
/// 2. 作为 summer 组件注册：`app.add_component(RateLimitEngine::new(...))`
///
/// # 语法
///
/// ```plain
/// #[rate_limit(rate = <u64>, per = "second|minute|hour|day", ...)]
/// ```
///
/// # 参数
///
/// - `rate` (**required**): 每个窗口允许的请求数
/// - `per` (**required**): 窗口大小，支持 `"second" | "minute" | "hour" | "day"`
/// - `key` (*optional*): 限流 key 类型，默认 `"global"`
///   - `"global"`：全局（所有请求共享）
///   - `"ip"`：按客户端 IP
///   - `"user"`：按用户（未登录时回退到 IP）
///   - `"header:<name>"`：按某个 Header 值（缺失时用 `"unknown"`）
/// - `backend` (*optional*): `"memory" | "redis"`，默认 `"memory"`
/// - `algorithm` (*optional*): 默认 `"token_bucket"`
///   - `"token_bucket" | "fixed_window" | "sliding_window" | "leaky_bucket" | "throttle_queue"`
/// - `failure_policy` (*optional*): 默认 `"fail_open"`
///   - `"fail_open" | "fail_closed" | "fallback_memory"`
/// - `burst` (*optional*): 仅 `token_bucket` 支持；默认等于 `rate`
/// - `max_wait_ms` (*optional*): 仅 `throttle_queue` 支持且必须提供；最大排队等待时间（毫秒）
/// - `message` (*optional*): 被限流时返回的提示文案（默认 `"请求过于频繁"`）
///
/// # 示例
///
/// ```rust,ignore
/// use summer_admin_macros::rate_limit;
/// use summer_common::error::ApiResult;
/// use summer_web::get_api;
///
/// // 单 IP 每秒 2 次
/// #[rate_limit(rate = 2, per = "second", key = "ip")]
/// #[get_api("/limited")]
/// async fn limited_handler() -> ApiResult<()> {
///     Ok(())
/// }
///
/// // 对登录用户隔离（未登录回退到 IP）
/// #[rate_limit(rate = 1, per = "second", key = "user")]
/// #[get_api("/user-limited")]
/// async fn user_limited_handler() -> ApiResult<()> {
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn rate_limit(args: TokenStream, input: TokenStream) -> TokenStream {
    rate_limit_macro::expand(args, input)
}
