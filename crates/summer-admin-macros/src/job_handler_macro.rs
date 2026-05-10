use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{Attribute, Expr, ItemFn, Lit, LitStr, Meta, parse_macro_input};

/// `#[job_handler("name")]` 把 async fn 注册为动态调度任务的 handler。
///
/// 约定签名：`async fn(ctx: JobContext) -> JobResult`。
///
/// 宏展开：
/// 1. 保留原 async fn
/// 2. 生成同名内部包装函数，把 `async fn` 转为 `fn(JobContext) -> Pin<Box<...>>`
/// 3. `inventory::submit!` 注册到 `summer_job_dynamic::JobHandlerEntry`，
///    同时把函数上的 `///` doc comment 作为 description 一并注册（前端下拉展示用）
pub fn expand(args: TokenStream, input: TokenStream) -> TokenStream {
    let name_lit = parse_macro_input!(args as LitStr);
    let item = parse_macro_input!(input as ItemFn);

    if item.sig.asyncness.is_none() {
        return syn::Error::new_spanned(item.sig.fn_token, "#[job_handler] requires async fn")
            .to_compile_error()
            .into();
    }

    let handler_name = name_lit.value();
    if handler_name.is_empty() {
        return syn::Error::new(Span::call_site(), "#[job_handler] name must not be empty")
            .to_compile_error()
            .into();
    }

    let description = extract_doc_comment(&item.attrs);

    let fn_ident = item.sig.ident.clone();
    let wrapper_ident = format_ident!("__job_handler_wrapper_{}", fn_ident);

    let expanded = quote! {
        #item

        #[doc(hidden)]
        fn #wrapper_ident(
            ctx: ::summer_job_dynamic::JobContext,
        ) -> ::std::pin::Pin<::std::boxed::Box<
            dyn ::std::future::Future<Output = ::summer_job_dynamic::JobResult> + ::std::marker::Send,
        >> {
            ::std::boxed::Box::pin(async move { #fn_ident(ctx).await })
        }

        ::summer_job_dynamic::__inventory::submit! {
            ::summer_job_dynamic::JobHandlerEntry {
                name: #handler_name,
                description: #description,
                call: #wrapper_ident,
            }
        }
    };

    expanded.into()
}

/// 把函数上所有 `///` / `#[doc = "..."]` 收集成一个字符串字面量。
/// 每行去掉 rustdoc 的起始空格；无注释时返回空串。
fn extract_doc_comment(attrs: &[Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(expr_lit) = &nv.value
            && let Lit::Str(s) = &expr_lit.lit
        {
            let raw = s.value();
            let trimmed = raw.strip_prefix(' ').unwrap_or(&raw).to_string();
            lines.push(trimmed);
        }
    }
    lines.join("\n").trim().to_string()
}
