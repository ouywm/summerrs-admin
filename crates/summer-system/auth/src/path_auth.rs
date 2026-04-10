use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;

/// 路径认证配置
#[derive(Debug, Clone, Default)]
pub struct PathAuthConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl PathAuthConfig {
    /// 检查路径是否需要鉴权
    pub fn requires_auth(&self, path: &str) -> bool {
        if self.include.is_empty() {
            return false;
        }

        for pattern in &self.exclude {
            if Self::matches(pattern, path) {
                return false;
            }
        }

        for pattern in &self.include {
            if Self::matches(pattern, path) {
                return true;
            }
        }

        false
    }

    /// Ant 风格路径匹配
    fn matches(pattern: &str, path: &str) -> bool {
        if pattern == "/**" {
            return true;
        }

        if let Some(prefix) = pattern.strip_suffix("/**") {
            return path.starts_with(prefix);
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

        pattern == path
    }
}

/// `PathAuthBuilder` — 流式 API 构建路径认证规则
#[derive(Debug, Clone, Default)]
pub struct PathAuthBuilder {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl PathAuthBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn include(mut self, pattern: impl Into<String>) -> Self {
        self.include.push(pattern.into());
        self
    }

    pub fn include_all(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include.extend(patterns.into_iter().map(|p| p.into()));
        self
    }

    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.exclude.push(pattern.into());
        self
    }

    pub fn exclude_all(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude.extend(patterns.into_iter().map(|p| p.into()));
        self
    }

    /// 是否已配置 include 规则
    pub fn is_configured(&self) -> bool {
        !self.include.is_empty()
    }

    /// 构建最终配置
    pub fn build(self) -> PathAuthConfig {
        PathAuthConfig {
            include: self.include,
            exclude: self.exclude,
        }
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
        let builder = configurator.configure_path_auth(PathAuthBuilder::new());
        if builder.is_configured() {
            self.add_component(builder.build())
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(include: &[&str], exclude: &[&str]) -> PathAuthConfig {
        PathAuthConfig {
            include: include.iter().map(|s| s.to_string()).collect(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn glob_star_star_matches_all() {
        let c = config(&["/**"], &[]);
        assert!(c.requires_auth("/"));
        assert!(c.requires_auth("/api/users"));
        assert!(c.requires_auth("/api/users/1/roles"));
    }

    #[test]
    fn prefix_glob_star_star() {
        let c = config(&["/api/**"], &[]);
        assert!(c.requires_auth("/api/users"));
        assert!(c.requires_auth("/api/users/1"));
        assert!(!c.requires_auth("/web/index"));
    }

    #[test]
    fn prefix_glob_single_star() {
        let c = config(&["/api/*"], &[]);
        assert!(c.requires_auth("/api/users"));
        assert!(!c.requires_auth("/api/users/1"));
    }

    #[test]
    fn prefix_glob_single_star_no_panic_on_exact_prefix() {
        let c = config(&["/api/*"], &[]);
        assert!(!c.requires_auth("/api"));
    }

    #[test]
    fn prefix_glob_single_star_trailing_slash() {
        let c = config(&["/api/*"], &[]);
        assert!(!c.requires_auth("/api/"));
    }

    #[test]
    fn suffix_match() {
        let c = config(&["*.json"], &[]);
        assert!(c.requires_auth("/meta/openapi.json"));
        assert!(!c.requires_auth("/meta/openapi.yaml"));
    }

    #[test]
    fn exact_match() {
        let c = config(&["/auth/login"], &[]);
        assert!(c.requires_auth("/auth/login"));
        assert!(!c.requires_auth("/auth/logout"));
    }

    #[test]
    fn exclude_has_priority() {
        let c = config(&["/**"], &["/auth/login"]);
        assert!(c.requires_auth("/api/user/list"));
        assert!(!c.requires_auth("/auth/login"));
    }

    #[test]
    fn empty_include_means_disabled() {
        let c = config(&[], &["/**"]);
        assert!(!c.requires_auth("/any/path"));
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

        assert!(config.requires_auth("/admin/dashboard"));
        assert!(config.requires_auth("/tenant/list"));
        assert!(!config.requires_auth("/auth/login"));
        assert!(!config.requires_auth("/public/health"));
    }
}
