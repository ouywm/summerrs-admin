use std::fmt;

use summer_web::axum::http;

/// Compile-time registered "public route" rule.
///
/// 通过 `inventory` 从 handler crate 用 `#[public]` / `#[no_auth]` 发射过来。
/// `group` 字段用于多鉴权域共存场景：每个 group 的 AuthLayer 只看自己 group
/// 下注册的 public route。旧调用点发射的条目 `group == ""`，兼容语义。
#[derive(Clone, Copy)]
pub struct PublicRoute {
    pub group: &'static str,
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

/// 只返回指定 group 下注册的公共路由（含 `group == ""` 的旧条目视情况处理）。
///
/// 返 `Vec` 而不是 `impl Iterator`：启动期一次性收集，调用端一般想多次迭代。
pub fn public_routes_in_group(group: &str) -> Vec<&'static PublicRoute> {
    iter_public_routes()
        .into_iter()
        .filter(|r| r.group == group)
        .collect()
}

#[macro_export]
macro_rules! register_public_route {
    // 旧调用形式 —— `(pattern)` 仅路径：group 留空，method=Any。
    ($pattern:literal) => {
        $crate::register_public_route!("", $crate::public_routes::MethodTag::Any, $pattern);
    };
    // 旧调用形式 —— `(method, pattern)`：group 留空。
    ($method:expr, $pattern:literal) => {
        $crate::register_public_route!("", $method, $pattern);
    };
    // 新调用形式 —— `(group, method, pattern)`：带 group 标签。
    ($group:expr, $method:expr, $pattern:literal) => {
        $crate::inventory::submit! {
            $crate::public_routes::PublicRoute{
                group: $group,
                method: $method,
                pattern: $pattern,
            }
        }
    };
}
