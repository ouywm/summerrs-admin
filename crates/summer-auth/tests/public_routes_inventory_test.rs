use summer_auth::path_auth::{PathAuthBuilder, PathAuthConfig, RouteRule};
use summer_auth::public_routes::{MethodTag, iter_public_routes};
use summer_web::axum::http;

summer_auth::register_public_route!(MethodTag::Post, "/auth/login");
summer_auth::register_public_route!(MethodTag::Any, "/public/file/**");

fn merge_public_into(mut cfg: PathAuthConfig) -> PathAuthConfig {
    for r in iter_public_routes() {
        let rule = RouteRule::new(r.method, r.pattern.to_string());
        if !cfg.exclude.contains(&rule) {
            cfg.exclude.push(rule);
        }
    }
    cfg
}

#[test]
fn inventory_public_routes_are_merged_as_method_specific_excludes() {
    let cfg = PathAuthBuilder::new().include("/**").build();
    let cfg = merge_public_into(cfg);

    // method-specific exclude
    assert!(cfg.requires_auth(&http::Method::GET, "/auth/login"));
    assert!(!cfg.requires_auth(&http::Method::POST, "/auth/login"));

    // method-agnostic exclude
    assert!(!cfg.requires_auth(&http::Method::GET, "/public/file/abc"));
    assert!(!cfg.requires_auth(&http::Method::POST, "/public/file/abc"));
}
