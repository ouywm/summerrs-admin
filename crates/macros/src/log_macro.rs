use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemFn, LitBool, LitStr, ReturnType, Token};

/// 操作类型枚举，对应数据库 business_type 字段
///
/// Other=0, Create=1, Update=2, Delete=3, Query=4, Export=5, Import=6, Auth=7
#[derive(Debug, Clone, Copy)]
pub enum BusinessType {
    Other = 0,
    Create = 1,
    Update = 2,
    Delete = 3,
    Query = 4,
    Export = 5,
    Import = 6,
    Auth = 7,
}

impl BusinessType {
    /// 从标识符解析，如 `Create`、`Delete`
    fn from_ident(ident: &Ident) -> syn::Result<Self> {
        match ident.to_string().as_str() {
            "Other" => Ok(Self::Other),
            "Create" => Ok(Self::Create),
            "Update" => Ok(Self::Update),
            "Delete" => Ok(Self::Delete),
            "Query" => Ok(Self::Query),
            "Export" => Ok(Self::Export),
            "Import" => Ok(Self::Import),
            "Auth" => Ok(Self::Auth),
            _ => Err(syn::Error::new(
                ident.span(),
                "无效的 biz_type，可选值: Other, Create, Update, Delete, Query, Export, Import, Auth",
            )),
        }
    }
}

/// 生成实体枚举路径 `model::entity::sys_operation_log::BusinessType::Xxx`
impl quote::ToTokens for BusinessType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let variant = match self {
            Self::Other => quote! { Other },
            Self::Create => quote! { Create },
            Self::Update => quote! { Update },
            Self::Delete => quote! { Delete },
            Self::Query => quote! { Query },
            Self::Export => quote! { Export },
            Self::Import => quote! { Import },
            Self::Auth => quote! { Auth },
        };
        tokens.extend(quote! { model::entity::sys_operation_log::BusinessType::#variant });
    }
}

/// 宏参数结构体
///
/// 解析 `#[log(module = "用户管理", action = "创建用户", biz_type = Create)]`
#[derive(Debug)]
pub struct LogArgs {
    /// 业务模块（必填）
    pub module: String,
    /// 操作描述（必填）
    pub action: String,
    /// 操作类型（必填）
    pub biz_type: BusinessType,
    /// 是否记录请求参数（默认 true，敏感接口设为 false）
    pub save_params: bool,
    /// 是否记录响应内容（默认 true）
    pub save_response: bool,
}

/// 解析宏参数
///
/// 支持以下格式（顺序无关）：
/// ```text
/// module = "用户管理", action = "创建用户", biz_type = Create
/// module = "认证管理", action = "登录", biz_type = Other, save_params = false
/// ```
impl Parse for LogArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut module: Option<String> = None;
        let mut action: Option<String> = None;
        let mut biz_type: Option<BusinessType> = None;
        let mut save_params: Option<bool> = None;
        let mut save_response: Option<bool> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "module" => {
                    let value: LitStr = input.parse()?;
                    module = Some(value.value());
                }
                "action" => {
                    let value: LitStr = input.parse()?;
                    action = Some(value.value());
                }
                "biz_type" => {
                    let value: Ident = input.parse()?;
                    biz_type = Some(BusinessType::from_ident(&value)?);
                }
                "save_params" => {
                    let value: LitBool = input.parse()?;
                    save_params = Some(value.value());
                }
                "save_response" => {
                    let value: LitBool = input.parse()?;
                    save_response = Some(value.value());
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "未知参数 `{}`，支持: module, action, biz_type, save_params, save_response",
                            key
                        ),
                    ));
                }
            }

            // 跳过逗号分隔符
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(LogArgs {
            module: module.ok_or_else(|| input.error("缺少必填参数 `module`"))?,
            action: action.ok_or_else(|| input.error("缺少必填参数 `action`"))?,
            biz_type: biz_type.ok_or_else(|| input.error("缺少必填参数 `biz_type`"))?,
            save_params: save_params.unwrap_or(true),
            save_response: save_response.unwrap_or(true),
        })
    }
}


/// 宏展开核心逻辑
///
/// 将 `#[log(...)]` 标注的 handler 函数转换为带有操作日志记录的函数。
///
/// 转换过程：
/// 1. 在函数参数列表前注入单一 `OperationLogContext` 提取器（内部合并 Method、Uri、HeaderMap、ClientIp、LoginId、Service）
/// 2. 将原始函数体包装在 `AssertUnwindSafe(async { ... }).catch_unwind().await` 中，同时捕获业务错误和 panic
/// 3. 根据执行结果提取操作状态（1=成功, 2=失败, 3=异常），记录请求信息、响应结果、耗时等，通过 tokio::spawn 异步写入数据库
/// 4. 返回原始执行结果；若为 panic 则 `resume_unwind` 恢复原始 panic，不影响业务逻辑
pub fn expand(args: TokenStream, input: TokenStream) -> TokenStream {
    // 1. 解析宏参数
    let log_args = match syn::parse2::<LogArgs>(args) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error(),
    };

    // 2. 解析 handler 函数
    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    // 3. 提取宏参数
    let module = &log_args.module;
    let action = &log_args.action;
    let biz_type = &log_args.biz_type;
    let save_params = log_args.save_params;
    let save_response = log_args.save_response;

    // 4. 提取函数各部分
    let attrs = &item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &item_fn.sig;
    let asyncness = &sig.asyncness;
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let where_clause = &sig.generics.where_clause;
    let original_inputs = &sig.inputs;
    let output = &sig.output;
    let original_body = &item_fn.block;

    // 5. 提取返回类型（用于 catch_unwind 结果类型标注，帮助编译器推断 async 块内的 Result 类型）
    let return_type = match output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    // 6. 根据 save_params 生成请求参数捕获代码
    let params_capture = if save_params {
        quote! {
            let __log_request_params: Option<serde_json::Value> = __log_query
                .map(|q| serde_json::Value::String(q));
        }
    } else {
        quote! {
            let __log_request_params: Option<serde_json::Value> = None;
        }
    };

    // 7. 根据 save_response 生成响应捕获代码（匹配 catch_unwind 三态结果）
    //    - Ok(Ok(resp))：成功，序列化响应体
    //    - Ok(Err(e))：业务失败，构造 RFC 7807 ProblemDetails 风格 JSON
    //    - Err(panic)：异常，构造 status=500 的 ProblemDetails 风格 JSON
    let response_capture = if save_response {
        quote! {
            let __log_response_body: Option<serde_json::Value> = match &__log_catch_result {
                Ok(Ok(resp)) => serde_json::to_value(resp).ok(),
                Ok(Err(e)) => Some(serde_json::json!({
                    "type": "about:blank",
                    "title": "Error",
                    "status": __log_response_code,
                    "detail": format!("{:#}", e),
                    "instance": __log_request_url.as_str(),
                })),
                Err(_) => Some(serde_json::json!({
                    "type": "about:blank",
                    "title": "Internal Server Error",
                    "status": 500,
                    "detail": __log_error_msg.as_deref(),
                    "instance": __log_request_url.as_str(),
                })),
            };
        }
    } else {
        quote! {
            let __log_response_body: Option<serde_json::Value> = None;
        }
    };

    // 8. 生成转换后的函数（仅注入单一 OperationLogContext 提取器）
    quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            __log_ctx: crate::service::operation_log_service::OperationLogContext,
            #original_inputs
        ) #output #where_clause {
            use futures::FutureExt as _;

            let __log_start = std::time::Instant::now();

            // 解构上下文
            let crate::service::operation_log_service::OperationLogContext {
                method: __log_request_method,
                uri: __log_request_url,
                query: __log_query,
                user_agent: __log_user_agent,
                client_ip: __log_client_ip,
                user_id: __log_user_id,
                op_svc: __log_op_svc,
            } = __log_ctx;

            #params_capture

            // 执行原始 handler 逻辑，同时捕获业务错误（Err）和 panic
            let __log_catch_result: Result<#return_type, Box<dyn std::any::Any + Send>> =
                std::panic::AssertUnwindSafe(async #original_body)
                    .catch_unwind()
                    .await;

            let __log_duration = __log_start.elapsed().as_millis() as i64;

            // 提取操作状态（Success=成功, Failed=失败, Exception=异常）
            let (__log_status, __log_error_msg, __log_response_code) = match &__log_catch_result {
                Ok(Ok(_)) => (model::entity::sys_operation_log::OperationStatus::Success, None::<String>, 200i16),
                Ok(Err(e)) => (model::entity::sys_operation_log::OperationStatus::Failed, Some(format!("{:#}", e)), 500i16),
                Err(__log_panic) => {
                    let __log_panic_msg = if let Some(s) = __log_panic.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = __log_panic.downcast_ref::<&str>() {
                        s.to_string()
                    } else {
                        "unknown panic".to_string()
                    };
                    (model::entity::sys_operation_log::OperationStatus::Exception, Some(__log_panic_msg), 500i16)
                }
            };

            #response_capture

            // 异步记录操作日志（不阻塞响应）
            __log_op_svc.record_async(model::dto::operation_log::CreateOperationLogDto {
                user_id: __log_user_id,
                module: #module.to_string(),
                action: #action.to_string(),
                business_type: #biz_type,
                request_method: __log_request_method,
                request_url: __log_request_url,
                request_params: __log_request_params,
                response_body: __log_response_body,
                response_code: __log_response_code,
                client_ip: __log_client_ip,
                user_agent: __log_user_agent,
                status: __log_status,
                error_msg: __log_error_msg,
                duration: __log_duration,
            });

            // 返回结果；若为 panic 则恢复原始 panic
            match __log_catch_result {
                Ok(__log_result) => __log_result,
                Err(__log_panic_payload) => std::panic::resume_unwind(__log_panic_payload),
            }
        }
    }
}