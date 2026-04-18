use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemFn, LitInt, LitStr, Token, parse_macro_input};

pub struct RateLimitArgs {
    pub rate: u64,
    pub per: String,
    pub burst: Option<u64>,
    pub max_wait_ms: Option<u64>,
    pub key: String,
    pub backend: String,
    pub algorithm: String,
    pub failure_policy: String,
    pub message: String,
}

impl Parse for RateLimitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut rate = None;
        let mut per = None;
        let mut burst = None;
        let mut max_wait_ms = None;
        let mut key = None;
        let mut backend = None;
        let mut algorithm = None;
        let mut failure_policy = None;
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
                "max_wait_ms" => {
                    let value: LitInt = input.parse()?;
                    max_wait_ms = Some(value.base10_parse()?);
                }
                "key" => {
                    let value: LitStr = input.parse()?;
                    key = Some(value.value());
                }
                "backend" => {
                    let value: LitStr = input.parse()?;
                    backend = Some(value.value());
                }
                "algorithm" => {
                    let value: LitStr = input.parse()?;
                    algorithm = Some(value.value());
                }
                "failure_policy" => {
                    let value: LitStr = input.parse()?;
                    failure_policy = Some(value.value());
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
            max_wait_ms,
            key: key.unwrap_or_else(|| "global".to_string()),
            backend: backend.unwrap_or_else(|| "memory".to_string()),
            algorithm: algorithm.unwrap_or_else(|| "token_bucket".to_string()),
            failure_policy: failure_policy.unwrap_or_else(|| "fail_open".to_string()),
            message: message.unwrap_or_else(|| "请求过于频繁".to_string()),
        })
    }
}

fn validate_args(args: &RateLimitArgs) -> syn::Result<()> {
    match args.algorithm.as_str() {
        "token_bucket" if args.max_wait_ms.is_some() => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`max_wait_ms` is only supported for throttle_queue",
            ));
        }
        "token_bucket" => {}
        "fixed_window" | "sliding_window" | "leaky_bucket" => {
            if args.burst.is_some() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`burst` is only supported for token_bucket",
                ));
            }
            if args.max_wait_ms.is_some() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`max_wait_ms` is only supported for throttle_queue",
                ));
            }
        }
        "throttle_queue" | "queue" | "throttle" => {
            if args.burst.is_some() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`burst` is not supported for throttle_queue",
                ));
            }
            match args.max_wait_ms {
                Some(wait_ms) if wait_ms > 0 => {}
                _ => {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "`max_wait_ms` must be provided and greater than 0 for throttle_queue",
                    ));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub fn expand(args: TokenStream, input: TokenStream) -> TokenStream {
    let rl_args = parse_macro_input!(args as RateLimitArgs);
    if let Err(error) = validate_args(&rl_args) {
        return error.to_compile_error().into();
    }
    let item_fn = parse_macro_input!(input as ItemFn);

    if item_fn.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            item_fn.sig.fn_token,
            "#[rate_limit] can only be used on async functions",
        )
        .to_compile_error()
        .into();
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
            .to_compile_error()
            .into();
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
            .to_compile_error()
            .into();
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
            .to_compile_error()
            .into();
        }
    };

    let algorithm_token = match rl_args.algorithm.as_str() {
        "token_bucket" => quote! { summer_common::rate_limit::RateLimitAlgorithm::TokenBucket },
        "fixed_window" => quote! { summer_common::rate_limit::RateLimitAlgorithm::FixedWindow },
        "sliding_window" => quote! { summer_common::rate_limit::RateLimitAlgorithm::SlidingWindow },
        "leaky_bucket" => quote! { summer_common::rate_limit::RateLimitAlgorithm::LeakyBucket },
        "throttle_queue" | "queue" | "throttle" => {
            quote! { summer_common::rate_limit::RateLimitAlgorithm::ThrottleQueue }
        }
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "invalid `algorithm`, expected one of: token_bucket, fixed_window, sliding_window, leaky_bucket, throttle_queue",
            )
            .to_compile_error()
            .into();
        }
    };

    let failure_policy_token = match rl_args.failure_policy.as_str() {
        "fail_open" => quote! { summer_common::rate_limit::RateLimitFailurePolicy::FailOpen },
        "fail_closed" => quote! { summer_common::rate_limit::RateLimitFailurePolicy::FailClosed },
        "fallback_memory" => {
            quote! { summer_common::rate_limit::RateLimitFailurePolicy::FallbackMemory }
        }
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "invalid `failure_policy`, expected one of: fail_open, fail_closed, fallback_memory",
            )
            .to_compile_error()
            .into();
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
    let max_wait_ms = rl_args.max_wait_ms.unwrap_or_default();
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
                        algorithm: #algorithm_token,
                        failure_policy: #failure_policy_token,
                        max_wait_ms: #max_wait_ms as u64,
                    },
                    #message,
                ).await?;
            }

            #(#stmts)*
        }
    }
    .into()
}
