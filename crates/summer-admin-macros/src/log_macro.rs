use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, ExprLit, FnArg, Ident, ItemFn, Lit, LitBool, LitStr, Meta, MetaNameValue, Pat,
    ReturnType, Token, parse_macro_input, parse_quote,
};

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
    fn to_tokens(&self, tokens: &mut TokenStream2) {
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
        tokens.extend(
            quote! { summer_system_model::entity::sys_operation_log::BusinessType::#variant },
        );
    }
}

/// 宏参数结构体
///
/// 解析 `#[log(module = "用户管理", action = "创建用户", biz_type = Create)]`
#[derive(Debug)]
pub struct LogArgs {
    /// 业务模块（必填）
    /// TODO: 当前 `module` 同时承载了类似 `ai/渠道管理` 的命名空间信息。
    /// 后续如需更稳定地按域筛选/统计，应该新增独立 `domain` 字段，而不是继续混用在 `module` 中。
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

/// 从函数参数列表中查找 `Json(xxx)` 或 `ValidatedJson(xxx)` 模式，返回内部变量名 `xxx`
///
/// 匹配 `Json(dto): Json<T>` 和 `ValidatedJson(dto): ValidatedJson<T>` 两种提取器。
/// 仅匹配路径末段为 `Json` 或 `ValidatedJson` 的 TupleStruct 模式。
fn find_json_body_ident(inputs: &syn::punctuated::Punctuated<FnArg, Token![,]>) -> Option<Ident> {
    for arg in inputs {
        let FnArg::Typed(pat_type) = arg else {
            continue;
        };
        let Pat::TupleStruct(tuple_struct) = pat_type.pat.as_ref() else {
            continue;
        };
        // 检查路径末段是否为 Json 或 ValidatedJson
        let Some(last_seg) = tuple_struct.path.segments.last() else {
            continue;
        };
        let name = last_seg.ident.to_string();
        if name != "Json" && name != "ValidatedJson" {
            continue;
        }
        // 提取括号内第一个元素的标识符
        if let Some(Pat::Ident(pat_ident)) = tuple_struct.elems.first() {
            return Some(pat_ident.ident.clone());
        }
    }
    None
}

/// 从 `#[doc = "..."]` 属性中提取原始字符串内容
fn doc_value(attr: &syn::Attribute) -> Option<String> {
    let Meta::NameValue(MetaNameValue { path, value, .. }) = &attr.meta else {
        return None;
    };
    if !path.is_ident("doc") {
        return None;
    }
    let Expr::Lit(ExprLit {
        lit: Lit::Str(s), ..
    }) = value
    else {
        return None;
    };
    Some(s.value())
}

fn first_non_empty_doc_line(attrs: &[syn::Attribute]) -> Option<(usize, String)> {
    attrs.iter().enumerate().find_map(|(i, attr)| {
        let trimmed = doc_value(attr)?.trim().to_string();
        (!trimmed.is_empty()).then_some((i, trimmed))
    })
}

fn has_doc_tag_value(attrs: &[syn::Attribute], expected: &str) -> bool {
    attrs.iter().any(|attr| {
        doc_value(attr).is_some_and(|v| {
            v.trim()
                .strip_prefix("@tag ")
                .is_some_and(|tag| tag.trim() == expected)
        })
    })
}

fn has_doc_line_value(attrs: &[syn::Attribute], expected: &str) -> bool {
    attrs
        .iter()
        .any(|attr| doc_value(attr).is_some_and(|v| v.trim() == expected))
}

/// 在 `attrs` 的指定位置插入一条 `#[doc = " {text}"]`
fn insert_doc_attr(attrs: &mut Vec<syn::Attribute>, pos: usize, text: &str) {
    let lit = LitStr::new(&format!(" {text}"), Span::call_site());
    attrs.insert(pos, parse_quote!(#[doc = #lit]));
}

fn ensure_doc_summary_and_module_tag(attrs: &mut Vec<syn::Attribute>, action: &str, module: &str) {
    // ── summary / action 行 ──
    match first_non_empty_doc_line(attrs) {
        // 没有任何 doc 内容 → 以 action 作为 summary 插到最前面
        None => {
            let pos = attrs
                .iter()
                .position(|a| a.path().is_ident("doc"))
                .unwrap_or(0);
            insert_doc_attr(attrs, pos, action);
        }
        // 首行是 @指令（如 @see）→ 在其前面补 summary
        Some((index, line)) if line.starts_with('@') => {
            insert_doc_attr(attrs, index, action);
        }
        // 有自定义 summary，且与 action 不同 → 在 summary 后补 action 描述行
        Some((index, line)) => {
            let normalized = line.trim_start_matches('#').trim();
            if normalized != action
                && !has_doc_line_value(attrs, action)
                && !has_doc_line_value(attrs, &format!("操作：{action}"))
            {
                insert_doc_attr(attrs, index + 1, action);
            }
        }
    }

    // ── @tag 行 ──
    if !has_doc_tag_value(attrs, module) {
        let pos = attrs
            .iter()
            .rposition(|a| a.path().is_ident("doc"))
            .map_or(0, |p| p + 1);
        insert_doc_attr(attrs, pos, &format!("@tag {module}"));
    }
}

fn ty_path_ends_with(ty: &syn::Type, expected: &[&str]) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let segs = &type_path.path.segments;
    segs.len() >= expected.len()
        && segs
            .iter()
            .rev()
            .zip(expected.iter().rev())
            .all(|(seg, &exp)| seg.ident == exp)
}

fn is_axum_response_ty(ty: &syn::Type) -> bool {
    ty_path_ends_with(ty, &["response", "Response"])
}

fn is_api_errors_ty(ty: &syn::Type) -> bool {
    ty_path_ends_with(ty, &["ApiErrors"])
}

/// 从返回类型 `Result<T, E>` / `ApiResult<T>` 中推断：
/// - `ok_is_response`：Ok 类型是否为 axum `Response`（需要从 resp.status() 取状态码）
/// - `err_is_api_errors`：Err 类型是否为 `ApiErrors`（可按变体映射 HTTP 状态码）
fn infer_log_status_code_strategy(output: &syn::ReturnType) -> (bool, bool) {
    /// 从 `ReturnType` 一路钻到最外层泛型的类型参数列表
    fn extract_result_parts(
        output: &syn::ReturnType,
    ) -> Option<(&syn::PathSegment, Vec<&syn::Type>)> {
        let ReturnType::Type(_, ty) = output else {
            return None;
        };
        let syn::Type::Path(tp) = ty.as_ref() else {
            return None;
        };
        let seg = tp.path.segments.last()?;
        let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
            return None;
        };
        let types: Vec<_> = args
            .args
            .iter()
            .filter_map(|a| match a {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect();
        Some((seg, types))
    }

    let Some((seg, types)) = extract_result_parts(output) else {
        return (false, false);
    };
    let Some(&ok_ty) = types.first() else {
        return (false, false);
    };

    let ok_is_response = is_axum_response_ty(ok_ty);

    let err_is_api_errors = match seg.ident.to_string().as_str() {
        "Result" => types.get(1).is_some_and(|ty| is_api_errors_ty(ty)),
        "ApiResult" => types.get(1).is_none_or(|ty| is_api_errors_ty(ty)),
        _ => false,
    };

    (ok_is_response, err_is_api_errors)
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
    let log_args = parse_macro_input!(args as LogArgs);

    // 2. 解析 handler 函数
    let mut item_fn = parse_macro_input!(input as ItemFn);

    // 3. 校验 #[log] 在路由宏上方（attrs 中应能看到未展开的 *_api）
    let has_api_attr = item_fn.attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident.to_string().ends_with("_api"))
    });
    if !has_api_attr {
        return syn::Error::new_spanned(
            &item_fn.sig.ident,
            "#[log] 必须放在路由宏（如 #[get_api]）的上方，否则生成的 doc 注释无法正确传递",
        )
        .to_compile_error()
        .into();
    }

    // 4. 提取宏参数
    let module = &log_args.module;
    let action = &log_args.action;
    let biz_type = &log_args.biz_type;
    let save_params = log_args.save_params;
    let save_response = log_args.save_response;

    // 5. 提取函数各部分
    let attrs = &mut item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &item_fn.sig;
    let asyncness = &sig.asyncness;
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let where_clause = &sig.generics.where_clause;
    let original_inputs = &sig.inputs;
    let output = &sig.output;
    let original_body = &item_fn.block;

    ensure_doc_summary_and_module_tag(attrs, action, module);

    let (ok_is_response, err_is_api_errors) = infer_log_status_code_strategy(output);

    // 6. 提取返回类型（用于 catch_unwind 结果类型标注，帮助编译器推断 async 块内的 Result 类型）
    let return_type = match output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    // 7. 查找 Json/ValidatedJson 请求体参数
    let json_body_ident = find_json_body_ident(original_inputs);

    // 8. 根据 save_params 生成请求参数捕获代码
    //    - 仅有 query：记录为 String
    //    - 仅有 body：序列化 DTO 为 JSON
    //    - 两者都有：合并为 {"query": "...", "body": {...}}
    let params_capture = if save_params {
        match json_body_ident {
            Some(body_ident) => {
                // 有 Json body，需要序列化 DTO（要求 T: Serialize）
                quote! {
                    let __log_body_value: Option<serde_json::Value> =
                        serde_json::to_value(&#body_ident).ok();

                    let __log_request_params: Option<serde_json::Value> = match (__log_query, __log_body_value) {
                        (Some(q), Some(b)) => Some(serde_json::json!({
                            "query": q,
                            "body": b,
                        })),
                        (None, Some(b)) => Some(b),
                        (Some(q), None) => Some(serde_json::Value::String(q)),
                        (None, None) => None,
                    };
                }
            }
            None => {
                // 没有 Json body，仅记录 query string
                quote! {
                    let __log_request_params: Option<serde_json::Value> = __log_query
                        .map(|q| serde_json::Value::String(q));
                }
            }
        }
    } else {
        quote! {
            let __log_request_params: Option<serde_json::Value> = None;
        }
    };

    // 9. 根据 save_response 生成响应捕获代码（匹配 catch_unwind 三态结果）
    //    - Ok(Ok(resp))：成功，序列化响应体（common::response::Json 实现了 Serialize）
    //    - Ok(Err(e))：业务失败，构造 ProblemDetails 并序列化
    //    - Err(panic)：异常，构造 status=500 的 ProblemDetails 并序列化
    let response_capture = if save_response {
        let ok_response_capture = if ok_is_response {
            quote! {
                Ok(Ok(_resp)) => None,
            }
        } else {
            quote! {
                Ok(Ok(resp)) => serde_json::to_value(resp).ok(),
            }
        };
        quote! {
            let __log_response_body: Option<serde_json::Value> = match &__log_catch_result {
                #ok_response_capture
                Ok(Err(e)) => {
                    let __log_pd = summer_web::problem_details::ProblemDetails::new(
                        "internal-error", "Internal Server Error", __log_response_code as u16,
                    )
                    .with_detail(format!("{:#}", e))
                    .with_instance(__log_request_url.clone());
                    serde_json::to_value(&__log_pd).ok()
                }
                Err(_) => {
                    let mut __log_pd = summer_web::problem_details::ProblemDetails::new(
                        "internal-error", "Internal Server Error", 500u16,
                    )
                    .with_instance(__log_request_url.clone());
                    if let Some(__log_msg) = __log_error_msg.as_deref() {
                        __log_pd = __log_pd.with_detail(__log_msg);
                    }
                    serde_json::to_value(&__log_pd).ok()
                }
            };
        }
    } else {
        quote! {
            let __log_response_body: Option<serde_json::Value> = None;
        }
    };

    // 10. 生成转换后的函数（仅注入单一 OperationLogContext 提取器）
    let ok_status_arm = if ok_is_response {
        quote! {
            Ok(Ok(resp)) => (
                summer_system_model::entity::sys_operation_log::OperationStatus::Success,
                None::<String>,
                resp.status().as_u16() as i16,
            ),
        }
    } else {
        quote! {
            Ok(Ok(_)) => (
                summer_system_model::entity::sys_operation_log::OperationStatus::Success,
                None::<String>,
                200i16,
            ),
        }
    };

    let err_status_arm = if err_is_api_errors {
        quote! {
            Ok(Err(e)) => {
                let __log_response_code = match e {
                    summer_common::error::ApiErrors::BadRequest(_) => 400i16,
                    summer_common::error::ApiErrors::Unauthorized(_) => 401i16,
                    summer_common::error::ApiErrors::Forbidden(_) => 403i16,
                    summer_common::error::ApiErrors::NotFound(_) => 404i16,
                    summer_common::error::ApiErrors::Conflict(_) => 409i16,
                    summer_common::error::ApiErrors::IncompleteUpload(_) => 409i16,
                    summer_common::error::ApiErrors::ValidationFailed(_) => 422i16,
                    summer_common::error::ApiErrors::PayloadTooLarge(_) => 413i16,
                    summer_common::error::ApiErrors::TooManyRequests(_) => 429i16,
                    summer_common::error::ApiErrors::Internal(_) => 500i16,
                    summer_common::error::ApiErrors::ServiceUnavailable(_) => 503i16,
                };
                (
                    summer_system_model::entity::sys_operation_log::OperationStatus::Failed,
                    Some(format!("{:#}", e)),
                    __log_response_code,
                )
            },
        }
    } else {
        quote! {
            Ok(Err(e)) => (
                summer_system_model::entity::sys_operation_log::OperationStatus::Failed,
                Some(format!("{:#}", e)),
                500i16,
            ),
        }
    };

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
                nick_name: __log_nick_name,
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
                #ok_status_arm
                #err_status_arm
                Err(__log_panic) => {
                    let __log_panic_msg = if let Some(s) = __log_panic.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = __log_panic.downcast_ref::<&str>() {
                        s.to_string()
                    } else {
                        "unknown panic".to_string()
                    };
                    (summer_system_model::entity::sys_operation_log::OperationStatus::Exception, Some(__log_panic_msg), 500i16)
                }
            };

            #response_capture

            // 异步记录操作日志（不阻塞响应）
            __log_op_svc.record_async(summer_system_model::dto::operation_log::CreateOperationLogDto {
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
            }, __log_nick_name);

            // 返回结果；若为 panic 则恢复原始 panic
            match __log_catch_result {
                Ok(__log_result) => __log_result,
                Err(__log_panic_payload) => std::panic::resume_unwind(__log_panic_payload),
            }
        }
    }.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Attribute, Expr, ExprLit, Lit, Meta, MetaNameValue, parse_quote};

    fn extract_doc_lines(attrs: &[Attribute]) -> Vec<String> {
        let mut lines = Vec::new();

        for attr in attrs {
            let Meta::NameValue(MetaNameValue { path, value, .. }) = &attr.meta else {
                continue;
            };
            if !path.is_ident("doc") {
                continue;
            }
            let Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) = value
            else {
                continue;
            };
            lines.push(s.value().trim_start().to_string());
        }

        while matches!(lines.first(), Some(l) if l.trim().is_empty()) {
            lines.remove(0);
        }

        lines
    }

    #[test]
    fn inserts_summary_and_tag_when_no_doc() {
        let mut attrs = Vec::<Attribute>::new();

        ensure_doc_summary_and_module_tag(&mut attrs, "查询操作日志", "操作日志");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec!["查询操作日志".to_string(), "@tag 操作日志".to_string()]
        );
    }

    #[test]
    fn keeps_user_summary_and_inserts_action_into_description() {
        let mut attrs: Vec<Attribute> =
            vec![parse_quote!(#[doc = " 单文件上传（multipart/form-data）"])];

        ensure_doc_summary_and_module_tag(&mut attrs, "上传文件", "文件管理");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec![
                "单文件上传（multipart/form-data）".to_string(),
                "上传文件".to_string(),
                "@tag 文件管理".to_string()
            ]
        );
    }

    #[test]
    fn inserts_action_summary_when_first_line_is_directive() {
        let mut attrs: Vec<Attribute> = vec![parse_quote!(#[doc = " @see https://example.com"])];

        ensure_doc_summary_and_module_tag(&mut attrs, "查询操作日志", "操作日志");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec![
                "查询操作日志".to_string(),
                "@see https://example.com".to_string(),
                "@tag 操作日志".to_string()
            ]
        );
    }

    #[test]
    fn does_not_duplicate_existing_module_tag() {
        let mut attrs: Vec<Attribute> = vec![
            parse_quote!(#[doc = " 单文件上传"]),
            parse_quote!(#[doc = " @tag 文件管理"]),
        ];

        ensure_doc_summary_and_module_tag(&mut attrs, "上传文件", "文件管理");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec![
                "单文件上传".to_string(),
                "上传文件".to_string(),
                "@tag 文件管理".to_string()
            ]
        );
    }

    #[test]
    fn does_not_insert_action_description_when_summary_equals_action() {
        let mut attrs: Vec<Attribute> = vec![parse_quote!(#[doc = " 查询操作日志"])];

        ensure_doc_summary_and_module_tag(&mut attrs, "查询操作日志", "操作日志");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec!["查询操作日志".to_string(), "@tag 操作日志".to_string()]
        );
    }

    #[test]
    fn does_not_duplicate_existing_action_description() {
        let mut attrs: Vec<Attribute> = vec![
            parse_quote!(#[doc = " 单文件上传"]),
            parse_quote!(#[doc = " 上传文件"]),
        ];

        ensure_doc_summary_and_module_tag(&mut attrs, "上传文件", "文件管理");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec![
                "单文件上传".to_string(),
                "上传文件".to_string(),
                "@tag 文件管理".to_string()
            ]
        );
    }

    #[test]
    fn does_not_duplicate_legacy_action_description() {
        let mut attrs: Vec<Attribute> = vec![
            parse_quote!(#[doc = " 单文件上传"]),
            parse_quote!(#[doc = " 操作：上传文件"]),
        ];

        ensure_doc_summary_and_module_tag(&mut attrs, "上传文件", "文件管理");

        assert_eq!(
            extract_doc_lines(&attrs),
            vec![
                "单文件上传".to_string(),
                "操作：上传文件".to_string(),
                "@tag 文件管理".to_string()
            ]
        );
    }
}
