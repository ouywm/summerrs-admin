use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;
use summer_web::axum::http;

use crate::public_routes::MethodTag;

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

/// 路径认证配置
#[derive(Debug, Clone)]
pub struct PathAuthConfig {
    pub include: Vec<RouteRule>,
    pub exclude: Vec<RouteRule>,
    param_route_cache: ParamRouteCache,
}

impl PathAuthConfig {
    pub(crate) fn new(include: Vec<RouteRule>, exclude: Vec<RouteRule>) -> Self {
        Self {
            param_route_cache: ParamRouteCache {
                routers: Arc::new(Self::build_param_routers(&include, &exclude)),
            },
            include,
            exclude,
        }
    }

    pub(crate) fn rebuild_param_route_cache(&mut self) {
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
            if !pattern.contains('{') || !pattern.contains('}') {
                continue;
            }

            let mut router = matchit::Router::new();
            if router.insert(pattern, ()).is_ok() {
                routers.insert(pattern.to_string(), Arc::new(router));
            }
        }
        routers
    }

    /// 检查路径是否需要鉴权
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

/// `PathAuthBuilder` — 流式 API 构建路径认证规则
#[derive(Debug, Clone, Default)]
pub struct PathAuthBuilder {
    pub include: Vec<RouteRule>,
    pub exclude: Vec<RouteRule>,
}

impl PathAuthBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn include(mut self, pattern: impl Into<String>) -> Self {
        self.include.push(RouteRule::any(pattern));
        self
    }

    pub fn include_all(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include
            .extend(patterns.into_iter().map(|p| RouteRule::any(p)));
        self
    }

    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.exclude.push(RouteRule::any(pattern));
        self
    }

    pub fn exclude_all(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude
            .extend(patterns.into_iter().map(|p| RouteRule::any(p)));
        self
    }

    pub fn include_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.include.push(RouteRule::new(method, pattern));
        self
    }

    pub fn exclude_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.exclude.push(RouteRule::new(method, pattern));
        self
    }

    /// 是否已配置 include 规则
    pub fn is_configured(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty()
    }

    /// 构建最终配置
    pub fn build(self) -> PathAuthConfig {
        PathAuthConfig::new(self.include, self.exclude)
    }

    /// 合并两份路径认证配置
    pub fn merge(mut self, other: PathAuthBuilder) -> Self {
        self.include.extend(other.include);
        self.exclude.extend(other.exclude);
        self
    }
}

/// 认证配置 trait
pub trait AuthConfigurator: Send + Sync + 'static {
    fn configure_path_auth(&self, auth: PathAuthBuilder) -> PathAuthBuilder;
}

impl AuthConfigurator for PathAuthBuilder {
    fn configure_path_auth(&self, auth: PathAuthBuilder) -> PathAuthBuilder {
        auth.merge(self.clone())
    }
}

/// 扩展 `AppBuilder` 的 trait
pub trait SummerAuthConfigurator {
    fn auth_configure<C>(&mut self, configurator: C) -> &mut Self
    where
        C: AuthConfigurator;
}

impl SummerAuthConfigurator for AppBuilder {
    fn auth_configure<C>(&mut self, configurator: C) -> &mut Self
    where
        C: AuthConfigurator,
    {
        let mut builder = configurator.configure_path_auth(PathAuthBuilder::new());
        if !builder.is_configured() {
            return self;
        }

        // secure-by-default: 只配置了 exclude 时，默认认为“其他都需要鉴权”
        if builder.include.is_empty() && !builder.exclude.is_empty() {
            builder.include.push(RouteRule::any("/**"));
        }

        self.add_component(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(include: &[(MethodTag, &str)], exclude: &[(MethodTag, &str)]) -> PathAuthConfig {
        PathAuthConfig::new(
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
    fn builder_merge_keeps_include_and_exclude() {
        let a = PathAuthBuilder::new()
            .include("/admin/**")
            .exclude("/auth/login");
        let b = PathAuthBuilder::new()
            .include("/tenant/**")
            .exclude("/public/**");
        let config = a.merge(b).build();

        assert!(config.requires_auth(&http::Method::GET, "/admin/dashboard"));
        assert!(config.requires_auth(&http::Method::GET, "/tenant/list"));
        assert!(!config.requires_auth(&http::Method::GET, "/auth/login"));
        assert!(!config.requires_auth(&http::Method::GET, "/public/health"));
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
}
