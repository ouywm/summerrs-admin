use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{ItemFn, LitStr, parse_macro_input};

/// `#[job_handler("name")]` 把 async fn 注册为动态调度任务的 handler。
///
/// 约定签名：`async fn(ctx: JobContext) -> JobResult`。
///
/// 宏展开：
/// 1. 保留原 async fn
/// 2. 生成同名内部包装函数，把 `async fn` 转为 `fn(JobContext) -> Pin<Box<...>>`
/// 3. `inventory::submit!` 注册到 `summer_job_dynamic::JobHandlerEntry`
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
                call: #wrapper_ident,
            }
        }
    };

    expanded.into()
}
