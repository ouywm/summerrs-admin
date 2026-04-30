use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use summer_web::axum::http;

use crate::public_routes::{MethodTag, public_routes_in_group};

#[derive(Clone, Default)]
struct ParamRouteCache {
    routers: Arc<HashMap<String, Arc<matchit::Router<()>>>>,
}

impl std::fmt::Debug for ParamRouteCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParamRouteCache")
            .field("size", &self.routers.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteRule {
    pub method: MethodTag,
    pub pattern: String,
}

impl RouteRule {
    pub fn any(pattern: impl Into<String>) -> Self {
        Self {
            method: MethodTag::Any,
            pattern: pattern.into(),
        }
    }

    pub fn new(method: MethodTag, pattern: impl Into<String>) -> Self {
        Self {
            method,
            pattern: pattern.into(),
        }
    }
}

/// 单个鉴权域的路径策略：哪些 path 进鉴权（`include`）、哪些豁免（`exclude`）。
///
/// 用流式 API 构建；写操作内部自动维护参数化路由缓存。`include` 为空时
/// [`Self::requires_auth`] 总返 `false`（视为该域鉴权关闭）。
///
/// # Example
///
/// ```ignore
/// use summer_auth::path_auth::PathAuthConfig;
/// use summer_auth::public_routes::MethodTag;
///
/// let cfg = PathAuthConfig::new()
///     .include("/api/admin/**")
///     .exclude_method(MethodTag::Post, "/api/admin/auth/login")
///     .extend_excludes_from_public_routes("summer-ai-admin");
/// ```
#[derive(Debug, Clone, Default)]
pub struct PathAuthConfig {
    include: Vec<RouteRule>,
    exclude: Vec<RouteRule>,
    param_route_cache: ParamRouteCache,
}

impl PathAuthConfig {
    /// 空配置；`include` 为空意味着 `requires_auth` 总返 `false`（鉴权关闭）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 从既有规则向量构造（反序列化、批量装载等场景）。
    pub fn from_rules(include: Vec<RouteRule>, exclude: Vec<RouteRule>) -> Self {
        let mut cfg = Self {
            include,
            exclude,
            param_route_cache: ParamRouteCache::default(),
        };
        cfg.rebuild_cache();
        cfg
    }

    /// 添加需要鉴权的路径模式（method = Any）。
    #[must_use]
    pub fn include(mut self, pattern: impl Into<String>) -> Self {
        self.push_include(RouteRule::any(pattern));
        self
    }

    /// 添加豁免鉴权的路径模式（method = Any）；重复规则会被跳过。
    #[must_use]
    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.push_exclude(RouteRule::any(pattern));
        self
    }

    /// 添加需要鉴权的路径模式（指定方法）。
    #[must_use]
    pub fn include_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.push_include(RouteRule::new(method, pattern));
        self
    }

    /// 添加豁免鉴权的路径模式（指定方法）；重复规则会被跳过。
    #[must_use]
    pub fn exclude_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.push_exclude(RouteRule::new(method, pattern));
        self
    }

    /// 把 inventory 中标注 `#[public]` / `#[no_auth]` 且 group 命中的路由合并到 `exclude`。
    /// 重复规则会被跳过。
    #[must_use]
    pub fn extend_excludes_from_public_routes(mut self, group: &str) -> Self {
        for r in public_routes_in_group(group) {
            self.push_exclude(RouteRule::new(r.method, r.pattern.to_string()));
        }
        self
    }

    fn push_include(&mut self, rule: RouteRule) {
        let needs_rebuild = is_param_pattern(&rule.pattern);
        self.include.push(rule);
        if needs_rebuild {
            self.rebuild_cache();
        }
    }

    fn push_exclude(&mut self, rule: RouteRule) {
        if self.exclude.contains(&rule) {
            return;
        }
        let needs_rebuild = is_param_pattern(&rule.pattern);
        self.exclude.push(rule);
        if needs_rebuild {
            self.rebuild_cache();
        }
    }

    fn rebuild_cache(&mut self) {
        self.param_route_cache = ParamRouteCache {
            routers: Arc::new(Self::build_param_routers(&self.include, &self.exclude)),
        };
    }

    fn build_param_routers(
        include: &[RouteRule],
        exclude: &[RouteRule],
    ) -> HashMap<String, Arc<matchit::Router<()>>> {
        let mut uniq = HashSet::<&str>::new();
        for r in include {
            uniq.insert(r.pattern.as_str());
        }
        for r in exclude {
            uniq.insert(r.pattern.as_str());
        }

        let mut routers = HashMap::new();
        for pattern in uniq {
            if !is_param_pattern(pattern) {
                continue;
            }

            let mut router = matchit::Router::new();
            if router.insert(pattern, ()).is_ok() {
                routers.insert(pattern.to_string(), Arc::new(router));
            }
        }
        routers
    }

    /// 检查路径是否需要鉴权。
    pub fn requires_auth(&self, method: &http::Method, path: &str) -> bool {
        if self.include.is_empty() {
            return false;
        }

        for rule in &self.exclude {
            if self.rule_matches(rule, method, path) {
                return false;
            }
        }

        for rule in &self.include {
            if self.rule_matches(rule, method, path) {
                return true;
            }
        }

        false
    }

    fn rule_matches(&self, rule: &RouteRule, req_method: &http::Method, path: &str) -> bool {
        if !rule.method.matches(req_method) {
            return false;
        }
        self.matches_pattern(&rule.pattern, path)
    }

    /// Ant 风格路径匹配
    fn matches_pattern(&self, pattern: &str, path: &str) -> bool {
        if pattern == "/**" {
            return true;
        }

        if let Some(prefix) = pattern.strip_suffix("/**") {
            if !path.starts_with(prefix) {
                return false;
            }
            let rest = &path[prefix.len()..];
            return rest.is_empty() || rest.starts_with('/');
        }

        if let Some(prefix) = pattern.strip_suffix("/*") {
            if !path.starts_with(prefix) {
                return false;
            }
            let rest = &path[prefix.len()..];
            if rest.len() < 2 || !rest.starts_with('/') {
                return false;
            }
            return !rest[1..].contains('/');
        }

        if pattern.starts_with("*.") {
            let suffix = &pattern[1..];
            return path.ends_with(suffix);
        }

        if let Some(router) = self.param_route_cache.routers.get(pattern) {
            return router.at(path).is_ok();
        }

        pattern == path
    }
}

fn is_param_pattern(pattern: &str) -> bool {
    pattern.contains('{') && pattern.contains('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(include: &[(MethodTag, &str)], exclude: &[(MethodTag, &str)]) -> PathAuthConfig {
        PathAuthConfig::from_rules(
            include
                .iter()
                .map(|(m, s)| RouteRule::new(*m, (*s).to_string()))
                .collect(),
            exclude
                .iter()
                .map(|(m, s)| RouteRule::new(*m, (*s).to_string()))
                .collect(),
        )
    }

    #[test]
    fn glob_star_star_matches_all() {
        let c = config(&[(MethodTag::Any, "/**")], &[]);
        assert!(c.requires_auth(&http::Method::GET, "/"));
        assert!(c.requires_auth(&http::Method::GET, "/api/users"));
        assert!(c.requires_auth(&http::Method::GET, "/api/users/1/roles"));
    }

    #[test]
    fn prefix_glob_star_star() {
        let c = config(&[(MethodTag::Any, "/api/**")], &[]);
        assert!(c.requires_auth(&http::Method::GET, "/api/users"));
        assert!(c.requires_auth(&http::Method::GET, "/api/users/1"));
        assert!(!c.requires_auth(&http::Method::GET, "/web/index"));
    }

    #[test]
    fn prefix_glob_star_star_requires_segment_boundary() {
        let c = config(&[(MethodTag::Any, "/api/**")], &[]);
        assert!(!c.requires_auth(&http::Method::GET, "/apiX"));
        assert!(!c.requires_auth(&http::Method::GET, "/apiX/users"));
    }

    #[test]
    fn prefix_glob_single_star() {
        let c = config(&[(MethodTag::Any, "/api/*")], &[]);
        assert!(c.requires_auth(&http::Method::GET, "/api/users"));
        assert!(!c.requires_auth(&http::Method::GET, "/api/users/1"));
    }

    #[test]
    fn prefix_glob_single_star_no_panic_on_exact_prefix() {
        let c = config(&[(MethodTag::Any, "/api/*")], &[]);
        assert!(!c.requires_auth(&http::Method::GET, "/api"));
    }

    #[test]
    fn prefix_glob_single_star_trailing_slash() {
        let c = config(&[(MethodTag::Any, "/api/*")], &[]);
        assert!(!c.requires_auth(&http::Method::GET, "/api/"));
    }

    #[test]
    fn suffix_match() {
        let c = config(&[(MethodTag::Any, "*.json")], &[]);
        assert!(c.requires_auth(&http::Method::GET, "/meta/openapi.json"));
        assert!(!c.requires_auth(&http::Method::GET, "/meta/openapi.yaml"));
    }

    #[test]
    fn exact_match() {
        let c = config(&[(MethodTag::Any, "/auth/login")], &[]);
        assert!(c.requires_auth(&http::Method::GET, "/auth/login"));
        assert!(!c.requires_auth(&http::Method::GET, "/auth/logout"));
    }

    #[test]
    fn exclude_has_priority() {
        let c = config(
            &[(MethodTag::Any, "/**")],
            &[(MethodTag::Any, "/auth/login")],
        );
        assert!(c.requires_auth(&http::Method::GET, "/api/user/list"));
        assert!(!c.requires_auth(&http::Method::GET, "/auth/login"));
    }

    #[test]
    fn empty_include_means_disabled() {
        let c = config(&[], &[(MethodTag::Any, "/**")]);
        assert!(!c.requires_auth(&http::Method::GET, "/any/path"));
    }

    #[test]
    fn method_specific_exclude() {
        let c = config(
            &[(MethodTag::Any, "/**")],
            &[(MethodTag::Post, "/auth/login")],
        );
        assert!(c.requires_auth(&http::Method::GET, "/auth/login"));
        assert!(!c.requires_auth(&http::Method::POST, "/auth/login"));
    }

    #[test]
    fn param_segment_matches_one_segment_only() {
        let c = config(
            &[(MethodTag::Any, "/**")],
            &[(MethodTag::Any, "/auth/sessions/{device}")],
        );
        assert!(!c.requires_auth(&http::Method::GET, "/auth/sessions/abc"));
        assert!(c.requires_auth(&http::Method::GET, "/auth/sessions/abc/test"));
    }

    #[test]
    fn fluent_builder_equivalent_to_from_rules() {
        let fluent = PathAuthConfig::new()
            .include("/**")
            .exclude_method(MethodTag::Post, "/auth/login");
        assert!(fluent.requires_auth(&http::Method::GET, "/auth/login"));
        assert!(!fluent.requires_auth(&http::Method::POST, "/auth/login"));
        assert!(fluent.requires_auth(&http::Method::GET, "/api/users"));
    }

    #[test]
    fn fluent_builder_dedupes_excludes() {
        let cfg = PathAuthConfig::new()
            .include("/**")
            .exclude("/healthz")
            .exclude("/healthz");
        // 仅做语义断言：重复 exclude 不影响匹配（push_exclude 会跳过）
        assert!(!cfg.requires_auth(&http::Method::GET, "/healthz"));
    }

    #[test]
    fn fluent_builder_param_pattern_rebuilds_cache() {
        let cfg = PathAuthConfig::new()
            .include("/**")
            .exclude("/users/{id}/public");
        assert!(!cfg.requires_auth(&http::Method::GET, "/users/123/public"));
        assert!(!cfg.requires_auth(&http::Method::GET, "/users/abc/public"));
        assert!(cfg.requires_auth(&http::Method::GET, "/users/123/private"));
    }

    #[test]
    fn multiple_configs_are_independent() {
        let a = PathAuthConfig::new().include("/admin/**");
        let b = PathAuthConfig::new().include("/api/**");
        assert!(a.requires_auth(&http::Method::GET, "/admin/users"));
        assert!(!a.requires_auth(&http::Method::GET, "/api/users"));
        assert!(b.requires_auth(&http::Method::GET, "/api/users"));
        assert!(!b.requires_auth(&http::Method::GET, "/admin/users"));
    }
}
