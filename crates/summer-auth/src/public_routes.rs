use std::fmt;

use summer_web::axum::http;

/// Compile-time registered "public route" rule.
///
/// This is collected via `inventory` from handler crates using proc-macros like `#[public]`.
#[derive(Clone, Copy)]
pub struct PublicRoute {
    pub method: MethodTag,
    pub pattern: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodTag {
    Any,
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Trace,
    Options,
}

impl MethodTag {
    pub fn matches(&self, method: &http::Method) -> bool {
        match self {
            Self::Any => true,
            Self::Get => method == http::Method::GET,
            Self::Post => method == http::Method::POST,
            Self::Put => method == http::Method::PUT,
            Self::Delete => method == http::Method::DELETE,
            Self::Patch => method == http::Method::PATCH,
            Self::Head => method == http::Method::HEAD,
            Self::Trace => method == http::Method::TRACE,
            Self::Options => method == http::Method::OPTIONS,
        }
    }
}

impl fmt::Display for MethodTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MethodTag::Any => "*",
            MethodTag::Get => "GET",
            MethodTag::Post => "POST",
            MethodTag::Put => "PUT",
            MethodTag::Delete => "DELETE",
            MethodTag::Patch => "PATCH",
            MethodTag::Head => "HEAD",
            MethodTag::Trace => "TRACE",
            MethodTag::Options => "OPTIONS",
        };
        f.write_str(s)
    }
}

inventory::collect!(PublicRoute);

pub fn iter_public_routes() -> inventory::iter<PublicRoute> {
    inventory::iter::<PublicRoute>
}

#[macro_export]
macro_rules! register_public_route {
    ($pattern:literal) => {
        $crate::register_public_route!($crate::public_routes::MethodTag::Any, $pattern);
    };
    ($method:expr, $pattern:literal) => {
        $crate::inventory::submit! {
            $crate::public_routes::PublicRoute{
                method: $method,
                pattern: $pattern,
            }
        }
    };
}
