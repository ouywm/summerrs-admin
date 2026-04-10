# Route And Service Patterns

This reference explains how route, DTO, VO, and service code is structured in the
current Summerrs Admin workspace.

## Canonical Examples

- Standard CRUD route: `crates/summer-system/src/router/sys_user.rs`
- Auth route: `crates/summer-system/src/router/auth.rs`
- Standard service: `crates/summer-system/src/service/sys_user_service.rs`
- Online user service: `crates/summer-system/src/service/online_service.rs`
- App assembly root: `crates/app/src/main.rs`

## Where Routes Live

- System routes: `crates/summer-system/src/router/*.rs`
- System services: `crates/summer-system/src/service/*.rs`
- DTO/VO contracts: `crates/summer-system/model/src/dto` and
  `crates/summer-system/model/src/vo`
- The app assembly root remains `crates/app/src/main.rs`

When you add a new route, the common flow is:

1. Add or update `crates/summer-system/src/router/<module>.rs`
2. Update `crates/summer-system/src/router/mod.rs`
3. Add or update the corresponding service in
   `crates/summer-system/src/service`
4. Keep `crates/app/src/main.rs` focused on plugin assembly, not business logic

## Route Macros

Prefer the OpenAPI-aware route macros already used in the repo:

- `get_api`
- `post_api`
- `put_api`
- `delete_api`

## Common Route Imports

```rust
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_admin_macros::log;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};
```

If login state is required, add:

```rust
use summer_auth::{AdminUser, LoginUser};
```

## Common Handler Parameters

Typical handler signatures combine:

- `Component(svc): Component<MyService>`
- `Path(id): Path<i64>`
- `Query(query): Query<MyQueryDto>`
- `ValidatedJson(dto): ValidatedJson<MyDto>`
- `pagination: Pagination`
- `AdminUser { login_id, profile, .. }: AdminUser`

## Common Return Shapes

With a response body:

```rust
pub async fn detail(...) -> ApiResult<Json<UserDetailVo>> {
    let vo = svc.get_user_detail(id).await?;
    Ok(Json(vo))
}
```

Without a response body:

```rust
pub async fn create(...) -> ApiResult<()> {
    svc.create_user(dto, operator).await?;
    Ok(())
}
```

Prefer `summer_common::response::Json<T>` over raw `axum::Json<T>`.

## `#[log]` Usage

Management routes typically include operation logging:

```rust
#[log(module = "User", action = "Update User", biz_type = Update)]
#[put_api("/user/{id}")]
pub async fn update_user(...) -> ApiResult<()> {
    ...
}
```

Rules:

- Put `#[log]` directly above the route macro
- Use `save_params = false` for sensitive endpoints when needed

## Service Pattern

```rust
#[derive(Clone, Service)]
pub struct MyService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    other: OtherService,
}
```

Common injected types:

- `DbConn`
- `SessionManager`
- Other `Service` types
- Components registered by plugins

## Service Responsibilities

- Transactions belong in services
- Aggregation and policy logic belong in services
- Routers should only extract parameters, log, and delegate
- Use `ApiErrors` / `ApiResult` consistently
- Add context to database errors with `.context("...")?`

## Pagination Pattern

This repo commonly uses `summer-sea-orm` pagination helpers:

```rust
let page = sys_user::Entity::find()
    .filter(query)
    .page(&self.db, &pagination)
    .await?;
```

This works best when the query DTO can be passed directly into `.filter(query)`.

## Anti-Patterns

- Do not put heavy business logic in routes
- Do not manipulate `ActiveModel` directly in handlers
- Do not scatter multi-table aggregation logic across handlers
