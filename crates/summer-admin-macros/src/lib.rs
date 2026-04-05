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
    log_macro::expand(args.into(), input.into()).into()
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
    auth_macro::expand_check_login(args.into(), input.into()).into()
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
    auth_macro::expand_check_permission(args.into(), input.into()).into()
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
    auth_macro::expand_check_role(args.into(), input.into()).into()
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
    auth_macro::expand_check_permissions(args.into(), input.into()).into()
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
    auth_macro::expand_check_roles(args.into(), input.into()).into()
}

/// 限流属性宏
///
/// 为 HTTP handler 注入 `RateLimitContext` 并在业务逻辑前执行声明式限流检查。
#[proc_macro_attribute]
pub fn rate_limit(args: TokenStream, input: TokenStream) -> TokenStream {
    rate_limit_macro::expand(args.into(), input.into()).into()
}
