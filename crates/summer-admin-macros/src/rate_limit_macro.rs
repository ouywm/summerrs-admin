use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemFn, LitInt, LitStr, Token};

pub struct RateLimitArgs {
    pub rate: u64,
    pub per: String,
    pub burst: Option<u64>,
    pub key: String,
    pub backend: String,
    pub message: String,
}

impl Parse for RateLimitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut rate = None;
        let mut per = None;
        let mut burst = None;
        let mut key = None;
        let mut backend = None;
        let mut message = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "rate" => {
                    let value: LitInt = input.parse()?;
                    rate = Some(value.base10_parse()?);
                }
                "per" => {
                    let value: LitStr = input.parse()?;
                    per = Some(value.value());
                }
                "burst" => {
                    let value: LitInt = input.parse()?;
                    burst = Some(value.base10_parse()?);
                }
                "key" => {
                    let value: LitStr = input.parse()?;
                    key = Some(value.value());
                }
                "backend" => {
                    let value: LitStr = input.parse()?;
                    backend = Some(value.value());
                }
                "message" => {
                    let value: LitStr = input.parse()?;
                    message = Some(value.value());
                }
                _ => return Err(syn::Error::new(ident.span(), "unknown rate_limit argument")),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            rate: rate.ok_or_else(|| input.error("missing `rate`"))?,
            per: per.ok_or_else(|| input.error("missing `per`"))?,
            burst,
            key: key.unwrap_or_else(|| "global".to_string()),
            backend: backend.unwrap_or_else(|| "memory".to_string()),
            message: message.unwrap_or_else(|| "请求过于频繁".to_string()),
        })
    }
}

pub fn expand(args: TokenStream, input: TokenStream) -> TokenStream {
    let rl_args = match syn::parse2::<RateLimitArgs>(args) {
        Ok(args) => args,
        Err(error) => return error.to_compile_error(),
    };
    let item_fn = match syn::parse2::<ItemFn>(input) {
        Ok(item_fn) => item_fn,
        Err(error) => return error.to_compile_error(),
    };

    if item_fn.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            item_fn.sig.fn_token,
            "#[rate_limit] can only be used on async functions",
        )
        .to_compile_error();
    }

    let per_token = match rl_args.per.as_str() {
        "second" => quote! { summer_common::rate_limit::RateLimitPer::Second },
        "minute" => quote! { summer_common::rate_limit::RateLimitPer::Minute },
        "hour" => quote! { summer_common::rate_limit::RateLimitPer::Hour },
        "day" => quote! { summer_common::rate_limit::RateLimitPer::Day },
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "invalid `per`, expected one of: second, minute, hour, day",
            )
            .to_compile_error();
        }
    };

    let key_token = match rl_args.key.as_str() {
        "global" => quote! { summer_common::rate_limit::RateLimitKeyType::Global },
        "ip" => quote! { summer_common::rate_limit::RateLimitKeyType::Ip },
        "user" => quote! { summer_common::rate_limit::RateLimitKeyType::User },
        key if key.starts_with("header:") => {
            let header_name = key.trim_start_matches("header:");
            quote! { summer_common::rate_limit::RateLimitKeyType::Header(#header_name) }
        }
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "invalid `key`, expected one of: global, ip, user, header:<name>",
            )
            .to_compile_error();
        }
    };

    let backend_token = match rl_args.backend.as_str() {
        "memory" => quote! { summer_common::rate_limit::RateLimitBackend::Memory },
        "redis" => quote! { summer_common::rate_limit::RateLimitBackend::Redis },
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "invalid `backend`, expected one of: memory, redis",
            )
            .to_compile_error();
        }
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

    let rate = rl_args.rate;
    let burst = rl_args.burst.unwrap_or(rate);
    let message = rl_args.message;

    quote! {
        #(#attrs)*
        #vis #asyncness fn #fn_name #generics(
            __rate_limit_ctx: summer_common::rate_limit::RateLimitContext,
            #original_inputs
        ) #output #where_clause {
            {
                let __rl_key = __rate_limit_ctx.extract_key(#key_token);
                __rate_limit_ctx.check(
                    &__rl_key,
                    summer_common::rate_limit::RateLimitConfig {
                        rate: #rate as u32,
                        per: #per_token,
                        burst: #burst as u32,
                        backend: #backend_token,
                    },
                    #message,
                ).await?;
            }

            #(#stmts)*
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_args() {
        let input: TokenStream = quote! { rate = 100, per = "second" };
        let args: RateLimitArgs = syn::parse2(input).expect("parse args");
        assert_eq!(args.rate, 100);
        assert_eq!(args.per, "second");
        assert_eq!(args.key, "global");
        assert_eq!(args.backend, "memory");
    }

    #[test]
    fn expand_injects_rate_limit_context() {
        let args = quote! { rate = 10, per = "second" };
        let input = quote! {
            pub async fn login() -> ApiResult<()> {
                Ok(())
            }
        };

        let expanded = expand(args, input).to_string();
        assert!(expanded.contains("__rate_limit_ctx"));
        assert!(expanded.contains("RateLimitContext"));
    }
}
