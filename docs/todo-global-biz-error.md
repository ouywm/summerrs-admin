# TODO: 全局业务错误抽象

## 目标

消除 router 层 `.map_err()` 中的字符串匹配（`contains`），通过类型安全的方式将业务错误映射到 HTTP 状态码。

## 设计

### 1. 定义全局业务错误枚举（service 层使用）

```rust
#[derive(Debug, thiserror::Error)]
pub enum BizError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    AlreadyExists(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    BadRequest(String),
}
```

- service 仍然返回 `anyhow::Result`
- 通过 `.into()` 将 `BizError` 转为 `anyhow::Error`

### 2. 统一映射到 KnownWebError

```rust
impl From<BizError> for KnownWebError {
    fn from(e: BizError) -> Self {
        match &e {
            BizError::NotFound(_) => KnownWebError::not_found(e.to_string()),
            BizError::AlreadyExists(_) => KnownWebError::bad_request(e.to_string()),
            BizError::Unauthorized(_) => KnownWebError::unauthorized(e.to_string()),
            BizError::Forbidden(_) => KnownWebError::forbidden(e.to_string()),
            BizError::BadRequest(_) => KnownWebError::bad_request(e.to_string()),
        }
    }
}
```

### 3. 辅助函数（router 层使用）

```rust
pub fn biz_err(e: anyhow::Error) -> KnownWebError {
    match e.downcast::<BizError>() {
        Ok(biz) => biz.into(),
        Err(e) => KnownWebError::internal_server_error(e.to_string()),
    }
}
```

### 4. 使用示例

service:
```rust
return Err(BizError::AlreadyExists(format!("用户名已存在: {}", dto.username)).into());
```

router:
```rust
let user = svc.create_user(dto).await.map_err(biz_err)?;
```

## 效果

- service 不依赖 spring-web
- router 错误处理压缩到一行
- 业务错误 -> BizError 变体决定 HTTP 状态码
- 非业务错误（DB 挂了等）-> 自动 500
- 编译期类型安全，不依赖字符串匹配


这是claude 输出
problem_details.rs — ProblemDetails 结构体

核心是 ProblemDetails 结构体（RFC 7807），5 个字段：

- type — 问题类型 URI，默认 "about:blank"
- title — 简短描述
- status — HTTP 状态码
- detail — 详细说明（可选）
- instance — 发生问题的请求路径（可选，middleware 自动填充）

IntoResponse 实现中做了两件事：
1. 检查 instance 是否为空，为空则从 tokio task-local 自动取请求 URI 填入
2. 以 application/problem+json 的 Content-Type 返回 JSON

capture_request_uri_middleware 就是在请求进来时把 URI 存到 task-local，供上面第 1 步使用。

OpenAPI 相关的函数（register_error_response_by_variant 等）是给 derive 宏生成的代码调用的，用于自动往 OpenAPI 文档里注册错误响应。

openapi-example/main.rs — 使用示例

展示了 #[derive(ProblemDetails)] 的完整用法：

- 定义枚举 ApiErrors，每个变体代表一种错误
- handler 返回 Result<Json<T>, ApiErrors>，直接 Err(ApiErrors::NotFoundError) 就行
- #[error(transparent)] 可以包装第三方错误（如 sqlx::Error），自动转为 500
- #[error("TeaPod error: {0:?}")] 变体可以携带自定义数据
- @status_codes doc 注解声明 handler 可能返回哪些错误，自动生成 OpenAPI 文档

关键区别在于：用了 ProblemDetails 之后，handler 不再需要 map_err、不需要字符串匹配，直接返回枚举变体，derive 宏自动处理 HTTP 状态码和 JSON 响应格式。