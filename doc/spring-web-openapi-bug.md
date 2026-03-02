# [Bug] All OpenAPI doc UIs (Swagger / Scalar / Redoc) fail to load when `global_prefix` is set

## Summary

When `global_prefix` is configured in `[web]` section, all OpenAPI documentation UIs — Swagger, Scalar, and Redoc — fail to load the OpenAPI spec because the fetch URL is missing the global prefix.

## Environment

- spring-web: 0.4.17
- aide: 0.16.0-alpha.2
- Rust edition: 2024

## Configuration

```toml
[web]
port = 8080
global_prefix = "/api"

[web.openapi]
doc_prefix = "/docs"
info = { title = "my-app", version = "0.0.1" }
```

## Steps to Reproduce

1. Configure `global_prefix = "/api"` and `doc_prefix = "/docs"` as above
2. Start the application
3. Visit any of the doc UIs:
   - `http://localhost:8080/api/docs/swagger`
   - `http://localhost:8080/api/docs/scalar`
   - `http://localhost:8080/api/docs/redoc`
4. All show the same error: **"Fetch error: Not Found /docs/openapi.json"**
5. But `http://localhost:8080/api/docs/openapi.json` returns 200 OK with valid JSON

## Root Cause

In `spring-web/src/lib.rs`, the `docs_routes()` function constructs `_openapi_path` without considering `global_prefix`:

```rust
pub fn docs_routes(OpenApiConfig { doc_prefix, info }: &OpenApiConfig) -> aide::axum::ApiRouter {
    let _openapi_path = &format!("{doc_prefix}/openapi.json");
    // Result: "/docs/openapi.json" — missing "/api" prefix
    let _doc_title = &info.title;

    // All three doc UIs receive the SAME incorrect path:

    #[cfg(feature = "openapi-scalar")]
    let router = router.route(
        "/scalar",
        aide::scalar::Scalar::new(_openapi_path)        // ← "/docs/openapi.json"
            .with_title(_doc_title)
            .axum_route(),
    );
    #[cfg(feature = "openapi-redoc")]
    let router = router.route(
        "/redoc",
        aide::redoc::Redoc::new(_openapi_path)          // ← "/docs/openapi.json"
            .with_title(_doc_title)
            .axum_route(),
    );
    #[cfg(feature = "openapi-swagger")]
    let router = router.route(
        "/swagger",
        aide::swagger::Swagger::new(_openapi_path)      // ← "/docs/openapi.json"
            .with_title(_doc_title)
            .axum_route(),
    );

    router.route("/openapi.json", axum::routing::get(serve_docs))
}
```

The route nesting order in `schedule()`:

```
1. nest_api_service("/docs", docs_routes())   → /docs/openapi.json, /docs/swagger, /docs/scalar, /docs/redoc
2. nest("/api", router)                       → /api/docs/openapi.json, /api/docs/swagger, ...
```

- **Actual route**: `GET /api/docs/openapi.json` → 200 OK
- **All doc UIs fetch**: `GET /docs/openapi.json` → 404 Not Found

The HTML pages are served correctly at `/api/docs/{swagger,scalar,redoc}`, but internally they all try to fetch `/docs/openapi.json` — without the `/api` prefix.

## Suggested Fix

Pass `global_prefix` into `docs_routes()` and prepend it to `_openapi_path`:

```rust
// 1. docs_routes accepts global_prefix
pub fn docs_routes(openapi_conf: &OpenApiConfig, global_prefix: &str) -> aide::axum::ApiRouter {
    let _openapi_path = &format!("{global_prefix}{doc_prefix}/openapi.json");
    // Now: "/api/docs/openapi.json" ✓
    // Scalar, Redoc, Swagger all receive the correct path
    // ...
}

// 2. finish_openapi passes it through
fn finish_openapi(
    app: &App,
    router: aide::axum::ApiRouter,
    openapi_conf: OpenApiConfig,
    global_prefix: &str,
) -> axum::Router {
    let router = router.nest_api_service(
        &openapi_conf.doc_prefix,
        docs_routes(&openapi_conf, global_prefix),
    );
    // ...
}

// 3. schedule() passes global_prefix from ServerConfig
let router = {
    let openapi_conf = app.get_expect_component::<OpenApiConfig>();
    finish_openapi(&app, router, openapi_conf, &config.global_prefix)
};
```

Minimal change — only the `_openapi_path` construction needs the prefix. All three doc UIs (Scalar, Redoc, Swagger) share this variable, so the fix covers all of them.