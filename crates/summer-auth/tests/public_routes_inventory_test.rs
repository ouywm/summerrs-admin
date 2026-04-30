use summer_auth::path_auth::PathAuthConfig;
use summer_auth::public_routes::MethodTag;
use summer_web::axum::http;

summer_auth::register_public_route!("test", MethodTag::Post, "/auth/login");
summer_auth::register_public_route!("test", MethodTag::Any, "/public/file/**");

#[test]
fn extend_excludes_from_public_routes_merges_inventory() {
    let cfg = PathAuthConfig::new()
        .include("/**")
        .extend_excludes_from_public_routes("test");

    // method-specific exclude
    assert!(cfg.requires_auth(&http::Method::GET, "/auth/login"));
    assert!(!cfg.requires_auth(&http::Method::POST, "/auth/login"));

    // method-agnostic exclude
    assert!(!cfg.requires_auth(&http::Method::GET, "/public/file/abc"));
    assert!(!cfg.requires_auth(&http::Method::POST, "/public/file/abc"));
}
