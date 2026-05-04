use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, ItemFn, LitStr, Token, parse_macro_input};

// ── 参数解析 ──

/// 单个字符串参数，用于 `#[has_perm("perm")]` / `#[has_role("role")]`
pub struct SingleArg {
    pub value: String,
}

impl Parse for SingleArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lit: LitStr = input.parse()?;
        Ok(SingleArg { value: lit.value() })
    }
}

/// 批量检查模式
#[derive(Debug, Clone, Copy)]
pub enum CheckMode {
    /// 全部满足
    And,
    /// 满足其一
    Or,
}

/// 多值参数，用于 `#[has_perms(and("a", "b"))]` / `#[has_roles(or("a", "b"))]`
pub struct MultiArgs {
    pub mode: CheckMode,
    pub values: Vec<String>,
}

impl Parse for MultiArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mode_ident: Ident = input.parse()?;
        let mode = match mode_ident.to_string().as_str() {
            "and" => CheckMode::And,
            "or" => CheckMode::Or,
            _ => {
                return Err(syn::Error::new(
                    mode_ident.span(),
                    "期望 `and` 或 `or`，如 #[has_perms(and(\"a\", \"b\"))]",
                ));
            }
        };

        let content;
        syn::parenthesized!(content in input);

        let punctuated: Punctuated<LitStr, Token![,]> = Punctuated::parse_terminated(&content)?;
        let values: Vec<String> = punctuated.iter().map(|lit| lit.value()).collect();

        if values.is_empty() {
            return Err(syn::Error::new(
                mode_ident.span(),
                "至少需要一个权限/角色值",
            ));
        }

        Ok(MultiArgs { mode, values })
    }
}

/// `#[public]` / `#[no_auth]` 参数：
/// - 空：自动从路由属性推导；group 默认 `env!("CARGO_PKG_NAME")`
/// - `"/path"`：method=Any
/// - `GET, "/path"`：指定 method+path
/// - 任一形式 + 可选 `, group = "xxx"`：显式指定 group
/// - 仅 `group = "xxx"`：group 显式，其他自动推导
pub struct PublicArgs {
    pub method: Option<String>,
    pub path: Option<String>,
    pub group: Option<String>,
}

impl Parse for PublicArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self {
                method: None,
                path: None,
                group: None,
            });
        }

        let mut method: Option<String> = None;
        let mut path: Option<String> = None;
        let mut group: Option<String> = None;

        // 仅 `group = "xxx"` 开头
        if input.peek(Ident) && input.peek2(Token![=]) {
            let k: Ident = input.parse()?;
            if k == "group" {
                let _: Token![=] = input.parse()?;
                let v: LitStr = input.parse()?;
                group = Some(v.value());
                return Ok(Self {
                    method,
                    path,
                    group,
                });
            }
            return Err(syn::Error::new(
                k.span(),
                "expected `group = \"...\"` or METHOD",
            ));
        }

        // `"/path"` 或 `METHOD, "/path"`
        if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            path = Some(lit.value());
        } else {
            let method_ident: Ident = input.parse()?;
            let _comma: Token![,] = input.parse()?;
            let path_lit: LitStr = input.parse()?;
            method = Some(method_ident.to_string());
            path = Some(path_lit.value());
        }

        // 可选尾巴 `, group = "xxx"`
        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let k: Ident = input.parse()?;
            if k != "group" {
                return Err(syn::Error::new(
                    k.span(),
                    "unknown key, only `group = \"...\"` is supported",
                ));
            }
            let _: Token![=] = input.parse()?;
            let v: LitStr = input.parse()?;
            group = Some(v.value());
        }

        Ok(Self {
            method,
            path,
            group,
        })
    }
}

// ── 宏展开 ──

/// `#[login]` — 注入 LoginUser 提取器确保已登录
///
/// 展开后在参数列表中注入 `_: summer_auth::LoginUser`，
/// 如果用户未登录，LoginUser 提取器会返回 401。
pub fn expand_check_login(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as ItemFn);

    let attrs = &item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &item_fn.sig;
    let asyncness = &sig.asyncness;
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let where_clause = &sig.generics.where_clause;
    let original_inputs = &sig.inputs;
    let output = &sig.output;
    let stmts = &item_fn.block.stmts;

    quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            _: summer_auth::LoginUser,
            #original_inputs
        ) #output #where_clause {
            #(#stmts)*
        }
    }
    .into()
}

/// `#[public]` / `#[no_auth]` — 注册公开路由到 inventory
pub fn expand_public_route(args: TokenStream, input: TokenStream) -> TokenStream {
    let arg = parse_macro_input!(args as PublicArgs);
    let item_fn = parse_macro_input!(input as ItemFn);

    let routes = match resolve_public_routes(
        &item_fn.attrs,
        arg.method.as_deref(),
        arg.path.as_deref(),
        arg.group.as_deref(),
    ) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let regs = routes.into_iter().map(|(group_expr, method_expr, path)| {
        let path_lit = LitStr::new(&path, proc_macro2::Span::call_site());
        quote! { summer_auth::register_public_route!(#group_expr, #method_expr, #path_lit); }
    });

    quote! {
        #item_fn
        #(#regs)*
    }
    .into()
}

#[derive(Debug)]
struct RouteMultiArgs {
    path: String,
    methods: Vec<String>,
    group: Option<String>,
}

impl Parse for RouteMultiArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path_lit: LitStr = input.parse()?;
        let path = path_lit.value();

        let mut methods = Vec::new();
        let mut group: Option<String> = None;
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let key: Ident = input.parse()?;

            // summer-web 的 route 宏支持 `debug` 这种无 `=` 的裸 key — 跳过即可。
            if !input.peek(Token![=]) {
                continue;
            }
            input.parse::<Token![=]>()?;

            if key == "method" {
                if input.peek(LitStr) {
                    let v: LitStr = input.parse()?;
                    methods.push(v.value());
                } else {
                    let v: Ident = input.parse()?;
                    methods.push(v.to_string());
                }
            } else if key == "group" {
                // summer-web 的 `#[post(..., group = "xxx")]` —— 我们需要这个值。
                let v: LitStr = input.parse()?;
                group = Some(v.value());
            } else {
                // 其他 key（summer-web 的 `transform`、或将来新增的元数据）
                // no_auth 用不到，吞掉 value 继续——而不是报错，避免"每加一个新参数
                // 就要改这里"。
                if input.peek(LitStr) {
                    let _: LitStr = input.parse()?;
                } else if input.peek(Ident) {
                    let _: Ident = input.parse()?;
                }
            }
        }

        Ok(Self {
            path,
            methods,
            group,
        })
    }
}

fn resolve_public_routes(
    attrs: &[Attribute],
    manual_method: Option<&str>,
    manual_path: Option<&str>,
    manual_group: Option<&str>,
) -> syn::Result<Vec<(TokenStream2, TokenStream2, String)>> {
    // 默认 group 等同 `TypedHandlerRegistrar::group()` 的默认实现，取调用点 crate 名。
    // 用 `env!` 宏展开期求值，结果是 `&'static str` 字面量。
    let default_group_expr = quote!(env!("CARGO_PKG_NAME"));
    let manual_group_expr = manual_group
        .map(|g| {
            let lit = LitStr::new(g, proc_macro2::Span::call_site());
            quote!(#lit)
        })
        .unwrap_or_else(|| default_group_expr.clone());

    // Manual path always wins.
    if let Some(path) = manual_path {
        let method_expr = if let Some(m) = manual_method {
            method_tag_expr(m)?
        } else {
            quote!(summer_auth::public_routes::MethodTag::Any)
        };
        return Ok(vec![(manual_group_expr, method_expr, path.to_string())]);
    }

    // Auto infer from route attrs.
    let mut found: Vec<(TokenStream2, TokenStream2, String)> = Vec::new();
    for attr in attrs {
        let ident = attr
            .path()
            .get_ident()
            .map(|i| i.to_string())
            .unwrap_or_default();

        macro_rules! route_attr_to_method_expr {
            ($s:expr) => {
                match $s {
                    // summer-web typed routes
                    "get_api" => Some(quote!(summer_auth::public_routes::MethodTag::Get)),
                    "post_api" => Some(quote!(summer_auth::public_routes::MethodTag::Post)),
                    "put_api" => Some(quote!(summer_auth::public_routes::MethodTag::Put)),
                    "delete_api" => Some(quote!(summer_auth::public_routes::MethodTag::Delete)),
                    "patch_api" => Some(quote!(summer_auth::public_routes::MethodTag::Patch)),
                    "head_api" => Some(quote!(summer_auth::public_routes::MethodTag::Head)),
                    "trace_api" => Some(quote!(summer_auth::public_routes::MethodTag::Trace)),
                    "options_api" => Some(quote!(summer_auth::public_routes::MethodTag::Options)),
                    "get" => Some(quote!(summer_auth::public_routes::MethodTag::Get)),
                    "post" => Some(quote!(summer_auth::public_routes::MethodTag::Post)),
                    "put" => Some(quote!(summer_auth::public_routes::MethodTag::Put)),
                    "delete" => Some(quote!(summer_auth::public_routes::MethodTag::Delete)),
                    "patch" => Some(quote!(summer_auth::public_routes::MethodTag::Patch)),
                    "head" => Some(quote!(summer_auth::public_routes::MethodTag::Head)),
                    "trace" => Some(quote!(summer_auth::public_routes::MethodTag::Trace)),
                    "options" => Some(quote!(summer_auth::public_routes::MethodTag::Options)),
                    _ => None,
                }
            };
        }

        let method_expr = route_attr_to_method_expr!(ident.as_str());

        if let Some(method_expr) = method_expr {
            // summer-web 的 `#[post("/x", group = "..", transform = "..")]` 允许在 path
            // 后面跟一串 key-value 参数。直接 `parse_args::<LitStr>()` 只接受裸字符串——
            // 这里用 `RouteMultiArgs` 兼容所有参数形式，我们只关心 path 和 group。
            let route_args: RouteMultiArgs = attr.parse_args().map_err(|_| {
                syn::Error::new_spanned(
                    attr,
                    "Failed to parse route path. Expected e.g. #[post(\"/path\")] or #[post(\"/path\", group = \"my-group\")].",
                )
            })?;
            let group_expr = if manual_group.is_some() {
                manual_group_expr.clone()
            } else if let Some(g) = route_args.group.as_deref() {
                let lit = LitStr::new(g, proc_macro2::Span::call_site());
                quote!(#lit)
            } else {
                default_group_expr.clone()
            };
            found.push((group_expr, method_expr, route_args.path));
            continue;
        }

        // route/api_route("/test", method="GET", method="HEAD")
        if ident == "route" || ident == "api_route" {
            let route_args: RouteMultiArgs = attr.parse_args().map_err(|_| {
                syn::Error::new_spanned(
                    attr,
                    "Failed to parse route(...) arguments. Expected e.g. #[route(\"/x\", method = \"GET\", method = \"HEAD\")].",
                )
            })?;
            if route_args.methods.is_empty() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "route/api_route requires at least one `method = ...` entry.",
                ));
            }
            let group_expr = if manual_group.is_some() {
                manual_group_expr.clone()
            } else if let Some(g) = route_args.group.as_deref() {
                let lit = LitStr::new(g, proc_macro2::Span::call_site());
                quote!(#lit)
            } else {
                default_group_expr.clone()
            };
            for m in route_args.methods {
                found.push((
                    group_expr.clone(),
                    method_tag_expr(&m)?,
                    route_args.path.clone(),
                ));
            }
        }
    }

    if found.is_empty() {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "No route attribute found for auto-infer. Put #[public] above #[get_api]/#[post_api]/... (or #[get]/#[post]/...), or use #[public(METHOD, \"/path\")].",
        ))
    } else {
        Ok(found)
    }
}

fn method_tag_expr(method: &str) -> syn::Result<TokenStream2> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Ok(quote!(summer_auth::public_routes::MethodTag::Get)),
        "POST" => Ok(quote!(summer_auth::public_routes::MethodTag::Post)),
        "PUT" => Ok(quote!(summer_auth::public_routes::MethodTag::Put)),
        "DELETE" => Ok(quote!(summer_auth::public_routes::MethodTag::Delete)),
        "PATCH" => Ok(quote!(summer_auth::public_routes::MethodTag::Patch)),
        "HEAD" => Ok(quote!(summer_auth::public_routes::MethodTag::Head)),
        "TRACE" => Ok(quote!(summer_auth::public_routes::MethodTag::Trace)),
        "OPTIONS" => Ok(quote!(summer_auth::public_routes::MethodTag::Options)),
        "*" | "ANY" => Ok(quote!(summer_auth::public_routes::MethodTag::Any)),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Unsupported method. Expected GET/POST/PUT/DELETE/PATCH/HEAD/TRACE/OPTIONS/ANY.",
        )),
    }
}

/// `#[has_perm("perm")]` — 单权限检查（支持通配符匹配）
pub fn expand_check_permission(args: TokenStream, input: TokenStream) -> TokenStream {
    let arg = parse_macro_input!(args as SingleArg);
    let item_fn = parse_macro_input!(input as ItemFn);

    let perm = &arg.value;
    let check_code = quote! {
        if !__auth_guard.permissions().iter().any(|__p| {
            summer_auth::permission_matches(__p, #perm)
        }) {
            tracing::info!("权限不足: {}", #perm);
            return Err(summer_common::error::ApiErrors::Forbidden(
                "无权限".to_string()
            ));
        }
    };

    wrap_with_guard(&item_fn, check_code).into()
}

/// `#[has_role("role")]` — 单角色检查
pub fn expand_check_role(args: TokenStream, input: TokenStream) -> TokenStream {
    let arg = parse_macro_input!(args as SingleArg);
    let item_fn = parse_macro_input!(input as ItemFn);

    let role = &arg.value;
    let check_code = quote! {
        if !__auth_guard.roles().iter().any(|__r| __r == #role) {
            tracing::info!("角色不足: {}", #role);
            return Err(summer_common::error::ApiErrors::Forbidden(
                "无权限".to_string()
            ));
        }
    };

    wrap_with_guard(&item_fn, check_code).into()
}

/// `#[has_perms(and("a", "b"))]` 或 `#[has_perms(or("a", "b"))]`
pub fn expand_check_permissions(args: TokenStream, input: TokenStream) -> TokenStream {
    let multi = parse_macro_input!(args as MultiArgs);
    let item_fn = parse_macro_input!(input as ItemFn);

    let values = &multi.values;
    let check_code = match multi.mode {
        CheckMode::And => {
            // 每个权限都必须匹配，不匹配时返回第一个不满足的
            quote! {
                let __user_perms = __auth_guard.permissions();
                #(
                    if !__user_perms.iter().any(|__p| summer_auth::permission_matches(__p, #values)) {
                        tracing::info!("权限不足: {}", #values);
                        return Err(summer_common::error::ApiErrors::Forbidden(
                            "无权限".to_string()
                        ));
                    }
                )*
            }
        }
        CheckMode::Or => {
            // 任一权限匹配即通过
            quote! {
                let __user_perms = __auth_guard.permissions();
                let __any_matched = [#(#values),*].iter().any(|__req| {
                    __user_perms.iter().any(|__p| summer_auth::permission_matches(__p, __req))
                });
                if !__any_matched {
                    tracing::info!("权限不足: {}", [#(#values),*].join(" | "));
                    return Err(summer_common::error::ApiErrors::Forbidden(
                        "无权限".to_string()
                    ));
                }
            }
        }
    };

    wrap_with_guard(&item_fn, check_code).into()
}

/// `#[has_roles(and("a", "b"))]` 或 `#[has_roles(or("a", "b"))]`
pub fn expand_check_roles(args: TokenStream, input: TokenStream) -> TokenStream {
    let multi = parse_macro_input!(args as MultiArgs);
    let item_fn = parse_macro_input!(input as ItemFn);

    let values = &multi.values;
    let check_code = match multi.mode {
        CheckMode::And => {
            quote! {
                let __user_roles = __auth_guard.roles();
                #(
                    if !__user_roles.iter().any(|__r| __r == #values) {
                        tracing::info!("角色不足: {}", #values);
                        return Err(summer_common::error::ApiErrors::Forbidden(
                            "无权限".to_string()
                        ));
                    }
                )*
            }
        }
        CheckMode::Or => {
            quote! {
                let __user_roles = __auth_guard.roles();
                let __any_matched = [#(#values),*].iter().any(|__req| {
                    __user_roles.iter().any(|__r| __r == __req)
                });
                if !__any_matched {
                    tracing::info!("角色不足: {}", [#(#values),*].join(" | "));
                    return Err(summer_common::error::ApiErrors::Forbidden(
                        "无权限".to_string()
                    ));
                }
            }
        }
    };

    wrap_with_guard(&item_fn, check_code).into()
}

// ── 内部辅助 ──

/// 在函数参数列表前注入 `__auth_guard: summer_auth::LoginUser`，
/// 并在函数体开头注入检查代码
fn wrap_with_guard(item_fn: &ItemFn, check_code: TokenStream2) -> TokenStream2 {
    let attrs = &item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &item_fn.sig;
    let asyncness = &sig.asyncness;
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let where_clause = &sig.generics.where_clause;
    let original_inputs = &sig.inputs;
    let output = &sig.output;
    let stmts = &item_fn.block.stmts;

    quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            __auth_guard: summer_auth::LoginUser,
            #original_inputs
        ) #output #where_clause {
            #check_code
            #(#stmts)*
        }
    }
}
