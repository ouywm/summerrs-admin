use std::collections::{HashMap, HashSet};
use std::sync::Arc;
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

/// 多 group 路径认证配置集合
#[derive(Debug, Clone, Default)]
pub struct PathAuthConfigs {
    inner: HashMap<&'static str, PathAuthConfig>,
}

impl PathAuthConfigs {
    pub fn get(&self, group: &str) -> Option<&PathAuthConfig> {
        self.inner.get(group)
    }

    pub fn get_mut(&mut self, group: &str) -> Option<&mut PathAuthConfig> {
        self.inner.get_mut(group)
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
    pub fn new(include: Vec<RouteRule>, exclude: Vec<RouteRule>) -> Self {
        Self {
            param_route_cache: ParamRouteCache {
                routers: Arc::new(Self::build_param_routers(&include, &exclude)),
            },
            include,
            exclude,
        }
    }

    pub fn rebuild_param_route_cache(&mut self) {
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
///
/// 多 crate 场景：每个 crate 通过 [`Self::group`] 标记所属 group，
/// 最终由 plugin 按 group 挂载不同的 auth layer。
#[derive(Debug, Clone, Default)]
pub struct PathAuthBuilderInner {
    pub include: Vec<RouteRule>,
    pub exclude: Vec<RouteRule>,
}

#[derive(Debug, Clone, Default)]
pub struct PathAuthBuilder {
    inner: HashMap<&'static str, PathAuthBuilderInner>,
}

/// 为单个 group 构建认证规则的流式 API
pub struct GroupAuthBuilder {
    group: &'static str,
    include: Vec<RouteRule>,
    exclude: Vec<RouteRule>,
}
impl GroupAuthBuilder {
    /// 添加需要认证的路径模式
    pub fn include(mut self, pattern: impl Into<String>) -> Self {
        self.include.push(RouteRule::any(pattern));
        self
    }

    /// 添加豁免认证的路径模式
    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.exclude.push(RouteRule::any(pattern));
        self
    }

    /// 添加需要认证的路径（指定方法）
    pub fn include_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.include.push(RouteRule::new(method, pattern));
        self
    }

    /// 添加豁免认证的路径（指定方法）
    pub fn exclude_method(mut self, method: MethodTag, pattern: impl Into<String>) -> Self {
        self.exclude.push(RouteRule::new(method, pattern));
        self
    }
}

impl PathAuthBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 为指定 group 添加认证规则
    pub fn group(name: &'static str) -> GroupAuthBuilder {
        GroupAuthBuilder {
            group: name,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
    /// 添加一个 group 的配置（使用 builder 模式）
    pub fn add_group(mut self, builder: GroupAuthBuilder) -> Self {
        self.inner.insert(
            builder.group,
            PathAuthBuilderInner {
                include: builder.include,
                exclude: builder.exclude,
            },
        );
        self
    }

    /// 构建所有配置
    pub fn build(self) -> PathAuthConfigs {
        let inner = self
            .inner
            .into_iter()
            .map(|(name, inner)| {
                let config = PathAuthConfig::new(inner.include.clone(), inner.exclude.clone());
                (name, config)
            })
            .collect();
        PathAuthConfigs { inner }
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
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
