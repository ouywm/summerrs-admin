mod log_macro;

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
/// pub async fn create_user(...) -> ApiResult<ApiResponse<()>> { ... }
///
/// #[log(module = "认证管理", action = "用户登录", biz_type = Other, save_params = false)]
/// #[post("/auth/login")]
/// pub async fn login(...) -> ApiResult<LoginVo> { ... }
/// ```
#[proc_macro_attribute]
pub fn log(args: TokenStream, input: TokenStream) -> TokenStream {
    log_macro::expand(args.into(), input.into()).into()
}