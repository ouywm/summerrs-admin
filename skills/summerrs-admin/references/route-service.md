# Route And Service Patterns

这部分把 `summer-web` 风格和当前仓库的 system 模块写法合并成一个 AI 可直接套用的模式。

## Canonical examples

- 标准 CRUD route：`crates/summer-system/src/router/sys_user.rs`
- 认证 route：`crates/summer-system/src/router/auth.rs`
- 标准 service：`crates/summer-system/src/service/sys_user_service.rs`
- 在线用户 service：`crates/summer-system/src/service/online_service.rs`
- 应用装配根：`crates/app/src/main.rs`

## Route 放哪里

- system 路由文件：`crates/summer-system/src/router/*.rs`
- system service：`crates/summer-system/src/service/*.rs`
- 应用装配仍在：`crates/app/src/main.rs`

新增 route 时，通常需要：

1. 新建或修改 `crates/summer-system/src/router/<module>.rs`
2. 更新 `crates/summer-system/src/router/mod.rs`
3. 如有新 service，更新 `crates/summer-system/src/service/mod.rs`
4. 保持 `crates/app/src/main.rs` 的装配逻辑不乱改

## Route 宏怎么选

本项目优先使用带 OpenAPI 的宏：

- `get_api`
- `post_api`
- `put_api`
- `delete_api`

## Route imports 模板

```rust
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_admin_macros::log;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};
```

如果需要登录态，再加：

```rust
use summer_auth::{AdminUser, LoginUser};
```

## Handler 参数怎么写

本仓库最常见的参数组合：

- `Component(svc): Component<MyService>`
- `Path(id): Path<i64>`
- `Query(query): Query<MyQueryDto>`
- `ValidatedJson(dto): ValidatedJson<MyDto>`
- `pagination: Pagination`
- `AdminUser { login_id, profile, .. }: AdminUser`

## Handler 返回值怎么写

### 有响应体

```rust
pub async fn detail(...) -> ApiResult<Json<UserDetailVo>> {
    let vo = svc.get_user_detail(id).await?;
    Ok(Json(vo))
}
```

### 无响应体

```rust
pub async fn create(...) -> ApiResult<()> {
    svc.create_user(dto, operator).await?;
    Ok(())
}
```

优先 `summer_common::response::Json<T>`，不要默认用裸 `axum::Json<T>`。

## `#[log]` 怎么用

当前管理接口几乎都带操作日志。模式如下：

```rust
#[log(module = "用户管理", action = "更新用户", biz_type = Update)]
#[put_api("/user/{id}")]
pub async fn update_user(...) -> ApiResult<()> {
    ...
}
```

规则：

- `#[log]` 放在 route 宏上方
- 敏感接口可以加 `save_params = false`

## Service 标准写法

```rust
#[derive(Clone, Service)]
pub struct MyService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    other: OtherService,
}
```

### 常见注入对象

- `DbConn`
- `SessionManager`
- 其他 `Service`
- 自定义插件注册的组件

## Service 内部约定

- 业务规则、事务、聚合查询写在 service
- router 只做参数提取和 service 转发
- 错误统一返回 `ApiErrors` / `ApiResult`
- 数据库错误通常 `.context("...")?`
- 复杂写操作优先事务包裹

## 分页查询模式

本仓库使用 `summer-sea-orm` 的分页扩展：

```rust
let page = sys_user::Entity::find()
    .filter(query)
    .page(&self.db, &pagination)
    .await?;
```

适用前提：

- Query DTO 能转成 `Condition`
- handler 参数里直接接 `Pagination`

## 手写接口的最小流程

1. 在 `dto` 定义请求结构和校验
2. 在 `vo` 定义返回结构和 `from_model()`
3. 在 `service` 写查询、事务、聚合逻辑
4. 在 `router` 只保留参数提取、日志、service 调用
5. 更新对应 `mod.rs`

## 反模式

- 不要把大量业务逻辑写在 route
- 不要让 route 直接操作 `ActiveModel`
- 不要把多表聚合逻辑散落在 handler
