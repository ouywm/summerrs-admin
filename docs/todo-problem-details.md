# TODO: 使用 ProblemDetails 替代 KnownWebError

## 什么是 ProblemDetails

RFC 7807 标准的结构化错误响应格式，返回 JSON 而不是纯文本：

```json
{
  "type": "about:blank",
  "title": "Resource Not Found",
  "status": 404,
  "detail": "The requested user was not found",
  "instance": "/api/sys-user/999"
}
```

Content-Type 为 `application/problem+json`。

## 与 KnownWebError 的区别

| | KnownWebError | ProblemDetails |
|---|---|---|
| 响应格式 | 纯文本 `(status_code, msg)` | 结构化 JSON |
| Content-Type | 默认 | `application/problem+json` |
| 自动填充请求 URI | 不支持 | `instance` 字段自动填充 |
| OpenAPI 文档集成 | 无 | `@status_codes` 注解自动生成 |
| 前端解析 | 需要自己约定格式 | 标准格式，通用解析 |

## 如何使用

### 1. 定义错误枚举

使用 `#[derive(ProblemDetails)]` 配合 `thiserror::Error`：

```rust
use spring_web::ProblemDetails as ProblemDetailsMacro;

#[derive(Debug, thiserror::Error, ProblemDetailsMacro)]
pub enum ApiErrors {
    // 最简用法：只需 status_code + error
    // error 内容会自动作为 title
    #[status_code(400)]
    #[error("Invalid input provided")]
    BadRequest,

    // 自定义 problem_type + title + detail
    #[status_code(400)]
    #[problem_type("https://api.myapp.com/problems/email-validation")]
    #[title("Email Validation Failed")]
    #[detail("The provided email address is not valid")]
    #[error("Invalid email")]
    InvalidEmail,

    // 404
    #[status_code(404)]
    #[error("Resource not found")]
    NotFoundError,

    // 401
    #[status_code(401)]
    #[problem_type("https://api.myapp.com/problems/authentication-required")]
    #[title("Authentication Required")]
    #[detail("You must be authenticated to access this resource")]
    #[instance("/auth/login")]
    #[error("Authentication required")]
    AuthenticationRequired,

    // 包装其他错误类型（transparent）
    #[status_code(500)]
    #[error(transparent)]
    DbError(#[from] sea_orm::DbErr),

    // 携带自定义数据的变体
    #[status_code(418)]
    #[error("TeaPod error occurred: {0:?}")]
    TeaPod(CustomErrorSchema),
}
```

### 2. 可用的属性

| 属性 | 必填 | 说明 |
|---|---|---|
| `#[status_code(code)]` | 是 | HTTP 状态码，如 400、404、500 |
| `#[error("...")]` | 是 | thiserror 的错误消息，无 `#[title]` 时自动作为 title |
| `#[problem_type("uri")]` | 否 | 问题类型 URI，默认 `"about:blank"` |
| `#[title("...")]` | 否 | 自定义 title，默认取 `#[error]` 的内容 |
| `#[detail("...")]` | 否 | 详细描述 |
| `#[instance("uri")]` | 否 | 问题实例 URI，默认由 middleware 自动填充请求路径 |

### 3. derive 宏自动生成的实现

- `From<ApiErrors> for ProblemDetails` — 错误类型转为 ProblemDetails 结构体
- `IntoResponse for ApiErrors` — 可直接作为 axum handler 的返回错误类型
- `ProblemDetailsVariantInfo` — OpenAPI 文档集成

### 4. handler 中使用

返回值类型改为 `Result<Json<T>, ApiErrors>`，直接返回枚举变体：

```rust
#[get_api("/user-info/{id}")]
async fn user_info_api(Path(id): Path<u32>) -> Result<Json<UserInfo>, ApiErrors> {
    match id {
        0 => Err(ApiErrors::BadRequest),
        999 => Err(ApiErrors::NotFoundError),
        _ => Ok(Json(fetch_user(id).await?)),
    }
}
```

注意：handler 宏用的是 `#[get_api]` 而不是 `#[get]`，这是 openapi 版本的路由宏。

### 5. OpenAPI 文档注解

在 handler 的 doc comment 中用 `@status_codes` 声明可能返回的错误，自动生成到 OpenAPI 文档：

```rust
/// @status_codes ApiErrors::BadRequest, ApiErrors::NotFoundError, ApiErrors::AuthenticationRequired
#[get_api("/user-info/{id}")]
async fn user_info_api(...) -> Result<Json<UserInfo>, ApiErrors> { ... }
```

### 6. instance 字段自动填充

spring-web 内置 `capture_request_uri_middleware`，会把请求 URI 存到 tokio task-local。
ProblemDetails 的 `IntoResponse` 实现中，如果 `instance` 为空，会自动从 task-local 取出请求 URI 填入。

例如请求 `GET /api/sys-user/999` 返回 404 时，响应自动包含：
```json
{
  "instance": "/api/sys-user/999"
}
```

## 迁移步骤（从当前 KnownWebError 方案）

1. 确保 spring-web 启用 `openapi` feature（已启用）
2. 定义 `ApiErrors` 枚举，覆盖所有业务错误场景
3. router handler 返回值改为 `Result<Json<T>, ApiErrors>`
4. router 宏从 `#[get]` 改为 `#[get_api]`（如需 OpenAPI 文档）
5. service 层不变，仍然返回 `anyhow::Result`
6. router 中 service 调用的 `.map_err()` 改为映射到 `ApiErrors` 变体

## 与全局 BizError 方案的关系

两个方案可以结合：
- `BizError`（见 todo-global-biz-error.md）定义 service 层的业务错误类型
- `ApiErrors`（ProblemDetails）定义 router 层的 HTTP 错误响应
- router 中通过 `downcast` BizError 然后转为对应的 ApiErrors 变体
