use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, ItemFn, LitStr, Token};

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
                ))
            }
        };

        let content;
        syn::parenthesized!(content in input);

        let punctuated: Punctuated<LitStr, Token![,]> =
            Punctuated::parse_terminated(&content)?;
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

// ── 宏展开 ──

/// `#[login]` — 注入 LoginUser 提取器确保已登录
///
/// 展开后在参数列表中注入 `_: summer_auth::LoginUser`，
/// 如果用户未登录，LoginUser 提取器会返回 401。
pub fn expand_check_login(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

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
}

/// `#[has_perm("perm")]` — 单权限检查（支持通配符匹配）
pub fn expand_check_permission(args: TokenStream, input: TokenStream) -> TokenStream {
    let arg = match syn::parse2::<SingleArg>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let perm = &arg.value;
    let check_code = quote! {
        if !__auth_guard.permissions().iter().any(|__p| {
            summer_auth::permission_matches(__p, #perm)
        }) {
            tracing::info!("权限不足: {}", #perm);
            return Err(common::error::ApiErrors::Forbidden(
                "无权限".to_string()
            ));
        }
    };

    wrap_with_guard(&item_fn, check_code)
}

/// `#[has_role("role")]` — 单角色检查
pub fn expand_check_role(args: TokenStream, input: TokenStream) -> TokenStream {
    let arg = match syn::parse2::<SingleArg>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let role = &arg.value;
    let check_code = quote! {
        if !__auth_guard.roles().iter().any(|__r| __r == #role) {
            tracing::info!("角色不足: {}", #role);
            return Err(common::error::ApiErrors::Forbidden(
                "无权限".to_string()
            ));
        }
    };

    wrap_with_guard(&item_fn, check_code)
}

/// `#[has_perms(and("a", "b"))]` 或 `#[has_perms(or("a", "b"))]`
pub fn expand_check_permissions(args: TokenStream, input: TokenStream) -> TokenStream {
    let multi = match syn::parse2::<MultiArgs>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let values = &multi.values;
    let check_code = match multi.mode {
        CheckMode::And => {
            // 每个权限都必须匹配，不匹配时返回第一个不满足的
            quote! {
                let __user_perms = __auth_guard.permissions();
                #(
                    if !__user_perms.iter().any(|__p| summer_auth::permission_matches(__p, #values)) {
                        tracing::info!("权限不足: {}", #values);
                        return Err(common::error::ApiErrors::Forbidden(
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
                    return Err(common::error::ApiErrors::Forbidden(
                        "无权限".to_string()
                    ));
                }
            }
        }
    };

    wrap_with_guard(&item_fn, check_code)
}

/// `#[has_roles(and("a", "b"))]` 或 `#[has_roles(or("a", "b"))]`
pub fn expand_check_roles(args: TokenStream, input: TokenStream) -> TokenStream {
    let multi = match syn::parse2::<MultiArgs>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let values = &multi.values;
    let check_code = match multi.mode {
        CheckMode::And => {
            quote! {
                let __user_roles = __auth_guard.roles();
                #(
                    if !__user_roles.iter().any(|__r| __r == #values) {
                        tracing::info!("角色不足: {}", #values);
                        return Err(common::error::ApiErrors::Forbidden(
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
                    return Err(common::error::ApiErrors::Forbidden(
                        "无权限".to_string()
                    ));
                }
            }
        }
    };

    wrap_with_guard(&item_fn, check_code)
}

// ── 内部辅助 ──

/// 在函数参数列表前注入 `__auth_guard: summer_auth::LoginUser`，
/// 并在函数体开头注入检查代码
fn wrap_with_guard(item_fn: &ItemFn, check_code: TokenStream) -> TokenStream {
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
