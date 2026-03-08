use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;

use crate::user_type::UserType;

/// 路径认证配置
#[derive(Debug, Clone, Default)]
pub struct PathAuthConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// 路径 → 允许的用户类型限制（空表示不限制类型）
    pub type_rules: Vec<(String, Vec<UserType>)>,
}

impl PathAuthConfig {
    /// 检查路径是否需要鉴权
    pub fn requires_auth(&self, path: &str) -> bool {
        // 如果没有 include 规则，默认不需要鉴权
        if self.include.is_empty() {
            return false;
        }

        // 检查是否在排除列表中
        for pattern in &self.exclude {
            if Self::matches(pattern, path) {
                return false;
            }
        }

        // 检查是否在包含列表中
        for pattern in &self.include {
            if Self::matches(pattern, path) {
                return true;
            }
        }

        false
    }

    /// 获取路径允许的用户类型（None 表示不限制，Some 表示只允许指定类型）
    pub fn allowed_user_types(&self, path: &str) -> Option<&[UserType]> {
        for (pattern, types) in &self.type_rules {
            if Self::matches(pattern, path) {
                return Some(types);
            }
        }
        None
    }

    /// Ant 风格路径匹配
    fn matches(pattern: &str, path: &str) -> bool {
        if pattern == "/**" {
            return true;
        }

        if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3];
            return path.starts_with(prefix);
        }

        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            if !path.starts_with(prefix) {
                return false;
            }
            let rest = &path[prefix.len()..];
            // rest 必须以 '/' 开头且只有一层（如 "/foo"，不能是 "/foo/bar"）
            if rest.len() < 2 || !rest.starts_with('/') {
                return false;
            }
            return !rest[1..].contains('/');
        }

        if pattern.starts_with("*.") {
            let suffix = &pattern[1..]; // 如 ".html"
            return path.ends_with(suffix);
        }

        // 精确匹配
        pattern == path
    }
}

/// PathAuthBuilder — 流式 API 构建路径认证规则
#[derive(Debug, Clone, Default)]
pub struct PathAuthBuilder {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub type_rules: Vec<(String, Vec<UserType>)>,
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

    /// 限制路径只允许指定的用户类型访问
    ///
    /// # Example
    /// ```rust,ignore
    /// PathAuthBuilder::new()
    ///     .include("/**")
    ///     .exclude("/auth/login")
    ///     .allow_types("/admin/**", &[UserType::Admin])
    ///     .allow_types("/api/customer/**", &[UserType::Customer, UserType::Admin])
    /// ```
    pub fn allow_types(mut self, pattern: impl Into<String>, types: &[UserType]) -> Self {
        self.type_rules.push((pattern.into(), types.to_vec()));
        self
    }

    pub fn is_configured(&self) -> bool {
        !self.include.is_empty()
    }

    pub fn build(self) -> PathAuthConfig {
        PathAuthConfig {
            include: self.include,
            exclude: self.exclude,
            type_rules: self.type_rules,
        }
    }

    pub fn merge(mut self, other: PathAuthBuilder) -> Self {
        self.include.extend(other.include);
        self.exclude.extend(other.exclude);
        self.type_rules.extend(other.type_rules);
        self
    }
}

/// 认证配置 trait
pub trait AuthConfigurator: Send + Sync + 'static {
    fn configure_path_auth(&self, auth: PathAuthBuilder) -> PathAuthBuilder;
}

/// 为 PathAuthBuilder 实现 AuthConfigurator（允许直接传 PathAuthBuilder）
impl AuthConfigurator for PathAuthBuilder {
    fn configure_path_auth(&self, auth: PathAuthBuilder) -> PathAuthBuilder {
        auth.merge(self.clone())
    }
}

/// 扩展 AppBuilder 的 trait
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
            let config = builder.build();
            self.add_component(config)
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
            type_rules: vec![],
        }
    }

    // ── /** 全通配 ──

    #[test]
    fn glob_star_star_matches_all() {
        let c = config(&["/**"], &[]);
        assert!(c.requires_auth("/"));
        assert!(c.requires_auth("/api/users"));
        assert!(c.requires_auth("/api/users/1/roles"));
    }

    // ── /prefix/** 前缀多层通配 ──

    #[test]
    fn prefix_glob_star_star() {
        let c = config(&["/api/**"], &[]);
        assert!(c.requires_auth("/api/users"));
        assert!(c.requires_auth("/api/users/1"));
        assert!(!c.requires_auth("/web/index"));
    }

    // ── /prefix/* 单层通配 ──

    #[test]
    fn prefix_glob_single_star() {
        let c = config(&["/api/*"], &[]);
        assert!(c.requires_auth("/api/users"));
        assert!(!c.requires_auth("/api/users/1")); // 多层不匹配
    }

    #[test]
    fn prefix_glob_single_star_no_panic_on_exact_prefix() {
        // 之前这里会 panic（rest 为空字符串，rest[1..] 越界）
        let c = config(&["/api/*"], &[]);
        assert!(!c.requires_auth("/api")); // 不匹配，不 panic
    }

    #[test]
    fn prefix_glob_single_star_trailing_slash() {
        let c = config(&["/api/*"], &[]);
        // "/api/" → rest="/", len=1 < 2 → false
        assert!(!c.requires_auth("/api/"));
    }

    // ── *.ext 后缀匹配 ──

    #[test]
    fn suffix_match() {
        let c = config(&["*.html"], &[]);
        assert!(c.requires_auth("/index.html"));
        assert!(c.requires_auth("/deep/path/page.html"));
        assert!(!c.requires_auth("/api/data.json"));
    }

    // ── 精确匹配 ──

    #[test]
    fn exact_match() {
        let c = config(&["/auth/login"], &[]);
        assert!(c.requires_auth("/auth/login"));
        assert!(!c.requires_auth("/auth/logout"));
    }

    // ── exclude 优先 ──

    #[test]
    fn exclude_overrides_include() {
        let c = config(&["/**"], &["/auth/login", "/auth/register"]);
        assert!(!c.requires_auth("/auth/login"));
        assert!(!c.requires_auth("/auth/register"));
        assert!(c.requires_auth("/api/users"));
    }

    // ── 空 include 不需要鉴权 ──

    #[test]
    fn empty_include_no_auth() {
        let c = config(&[], &[]);
        assert!(!c.requires_auth("/anything"));
    }

    // ── PathAuthBuilder ──

    #[test]
    fn builder_merge() {
        let a = PathAuthBuilder::new()
            .include("/**")
            .exclude("/public");
        let b = PathAuthBuilder::new().exclude("/health");
        let merged = a.merge(b);
        let config = merged.build();
        assert!(!config.requires_auth("/public"));
        assert!(!config.requires_auth("/health"));
        assert!(config.requires_auth("/api/data"));
    }

    // ── 多类型用户限制 ──

    #[test]
    fn allow_types_admin_only() {
        let config = PathAuthBuilder::new()
            .include("/**")
            .exclude("/auth/login")
            .allow_types("/admin/**", &[UserType::Admin])
            .build();

        // /admin/** 只允许 Admin
        assert_eq!(
            config.allowed_user_types("/admin/users"),
            Some([UserType::Admin].as_slice())
        );
        // 非 /admin/ 路径无类型限制
        assert_eq!(config.allowed_user_types("/api/data"), None);
    }

    #[test]
    fn allow_types_multiple() {
        let config = PathAuthBuilder::new()
            .include("/**")
            .allow_types("/api/customer/**", &[UserType::Customer, UserType::Admin])
            .allow_types("/api/admin/**", &[UserType::Admin])
            .build();

        let types = config.allowed_user_types("/api/customer/orders").unwrap();
        assert!(types.contains(&UserType::Customer));
        assert!(types.contains(&UserType::Admin));

        let types = config.allowed_user_types("/api/admin/users").unwrap();
        assert!(types.contains(&UserType::Admin));
        assert!(!types.contains(&UserType::Customer));
    }

    #[test]
    fn allow_types_no_restriction() {
        let config = PathAuthBuilder::new()
            .include("/**")
            .build();

        // 无 type_rules 时所有路径无类型限制
        assert_eq!(config.allowed_user_types("/any/path"), None);
    }

    #[test]
    fn allow_types_builder_merge() {
        let a = PathAuthBuilder::new()
            .include("/**")
            .allow_types("/admin/**", &[UserType::Admin]);
        let b = PathAuthBuilder::new()
            .allow_types("/biz/**", &[UserType::Business]);
        let config = a.merge(b).build();

        assert_eq!(
            config.allowed_user_types("/admin/dashboard"),
            Some([UserType::Admin].as_slice())
        );
        assert_eq!(
            config.allowed_user_types("/biz/orders"),
            Some([UserType::Business].as_slice())
        );
        assert_eq!(config.allowed_user_types("/public"), None);
    }
}
