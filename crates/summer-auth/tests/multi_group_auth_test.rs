use summer_auth::path_auth::PathAuthBuilder;
use summer_auth::public_routes::MethodTag;
use summer_web::axum::http;

#[test]
fn test_multi_group_configs() {
    // 构建多 group 配置
    let builder = PathAuthBuilder::new()
        .add_group(
            PathAuthBuilder::group("group-a")
                .include("/**")
                .exclude("/public/**"),
        )
        .add_group(
            PathAuthBuilder::group("group-b")
                .include("/api/**")
                .exclude("/api/health"),
        );

    let configs = builder.build();

    // 测试 group-a
    let cfg_a = configs.get("group-a").expect("group-a not found");
    assert!(cfg_a.requires_auth(&http::Method::GET, "/admin/users"));
    assert!(!cfg_a.requires_auth(&http::Method::GET, "/public/file"));

    // 测试 group-b
    let cfg_b = configs.get("group-b").expect("group-b not found");
    assert!(cfg_b.requires_auth(&http::Method::GET, "/api/users"));
    assert!(!cfg_b.requires_auth(&http::Method::GET, "/api/health"));
    assert!(!cfg_b.requires_auth(&http::Method::GET, "/other/path"));
}

#[test]
fn test_group_isolation() {
    // group-a 和 group-b 的配置互不影响
    let builder = PathAuthBuilder::new()
        .add_group(PathAuthBuilder::group("group-a").include("/admin/**"))
        .add_group(PathAuthBuilder::group("group-b").include("/api/**"));

    let configs = builder.build();

    let cfg_a = configs.get("group-a").unwrap();
    let cfg_b = configs.get("group-b").unwrap();

    // group-a 只管 /admin/**
    assert!(cfg_a.requires_auth(&http::Method::GET, "/admin/users"));
    assert!(!cfg_a.requires_auth(&http::Method::GET, "/api/users"));

    // group-b 只管 /api/**
    assert!(cfg_b.requires_auth(&http::Method::GET, "/api/users"));
    assert!(!cfg_b.requires_auth(&http::Method::GET, "/admin/users"));
}

#[test]
fn test_method_specific_rules() {
    let builder = PathAuthBuilder::new().add_group(
        PathAuthBuilder::group("test")
            .include("/**")
            .exclude_method(MethodTag::Post, "/auth/login"),
    );

    let configs = builder.build();
    let cfg = configs.get("test").unwrap();

    // POST /auth/login 不需要鉴权
    assert!(!cfg.requires_auth(&http::Method::POST, "/auth/login"));
    // GET /auth/login 需要鉴权
    assert!(cfg.requires_auth(&http::Method::GET, "/auth/login"));
}

#[test]
fn test_param_routes() {
    let builder = PathAuthBuilder::new().add_group(
        PathAuthBuilder::group("test")
            .include("/**")
            .exclude("/users/{id}/public"),
    );

    let configs = builder.build();
    let cfg = configs.get("test").unwrap();

    // 参数路由应该被排除
    assert!(!cfg.requires_auth(&http::Method::GET, "/users/123/public"));
    assert!(!cfg.requires_auth(&http::Method::GET, "/users/abc/public"));

    // 其他路径需要鉴权
    assert!(cfg.requires_auth(&http::Method::GET, "/users/123/private"));
}
