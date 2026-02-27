# summerrs-admin 开发规范

## 项目结构

```
summerrs-admin/
├── Cargo.toml                     # workspace 根，集中管理所有依赖版本（仅版本号，不含 features）
├── config/                        # 应用配置文件（spring-rs 按 SPRING_ENV 加载 profile）
│   ├── app.toml                   # 基础配置（所有环境共享）
│   ├── app-dev.toml               # SPRING_ENV=dev
│   ├── app-prod.toml              # SPRING_ENV=prod
│   └── app-test.toml              # SPRING_ENV=test
├── sql/                           # 数据库脚本
│   └── init.sql
├── doc/                           # 项目文档
├── crates/
│   ├── app/                       # bin crate - 程序入口 + 路由 + 服务
│   │   └── src/
│   │       ├── main.rs            # App 启动、插件注册
│   │       ├── router/            # 路由处理器（按资源分文件）
│   │       │   └── mod.rs
│   │       └── service/           # 业务逻辑层
│   │           └── mod.rs
│   ├── model/                     # lib crate - 数据模型
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── entity/            # SeaORM 实体
│   │       ├── dto/               # 请求参数
│   │       └── vo/                # 响应对象
│   └── common/                    # lib crate - 公共工具、统一错误、统一请求、统一响应
│       └── src/
│           ├── lib.rs
│           ├── error.rs           # ApiErrors + ApiResult
│           ├── request.rs         # PageQuery
│           └── response.rs        # ApiResponse + PageResponse
```

## 依赖方向

```
app → model → common
app → common
```

单向流动，不允许反向依赖。

## Cargo.toml 规范

- `[workspace.dependencies]` 中只定义版本号，**不含 features**
- 各子 crate 的 `[dependencies]` 中按需声明 features
- 所有 dependencies **按字母排序**

---

## 错误处理规范

### ApiErrors（common/src/error.rs）

统一错误枚举，基于 `ProblemDetails`（RFC 7807），错误响应自动生成标准 JSON：

```rust
#[derive(Debug, thiserror::Error, ProblemDetails)]
pub enum ApiErrors {
    #[status_code(400)]  BadRequest(String),
    #[status_code(401)]  Unauthorized(String),
    #[status_code(403)]  Forbidden(String),
    #[status_code(404)]  NotFound(String),
    #[status_code(409)]  Conflict(String),        // 资源冲突（重复用户名、乐观锁等）
    #[status_code(422)]  ValidationFailed(String), // 参数校验失败
    #[status_code(429)]  TooManyRequests(String),
    #[status_code(500)]  Internal(anyhow::Error),  // 基础设施错误，自动从 anyhow 转换
    #[status_code(503)]  ServiceUnavailable(String),
}
```

错误响应格式（HTTP 4xx/5xx，Content-Type: application/problem+json）：

```json
{
  "type": "about:blank",
  "title": "用户不存在",
  "status": 404,
  "detail": "用户不存在",
  "instance": "/api/user/999"
}
```

### 自定义业务错误码

通过 `#[problem_type]` 在 `type` 字段中承载业务标识，无需额外枚举：

```rust
#[status_code(401)]
#[problem_type("urn:error:token-expired")]
#[title("Token Expired")]
#[error("Token expired")]
TokenExpired,
```

前端根据 `type` 字段做业务分支，根据 `status` 做通用 HTTP 处理。

### ApiResult（类型别名）

```rust
pub type ApiResult<T, E = ApiErrors> = Result<T, E>;
```

- 默认错误类型为 `ApiErrors`，省去重复书写
- 可指定其他错误类型：`ApiResult<T, MyCustomError>`

---

## 响应包装规范

### ApiResponse（common/src/response.rs）

成功响应的统一包装，三个字段始终序列化，与前端 `BaseResponse` 完全一致：

```rust
pub struct ApiResponse<T: Serialize> {
    pub code: i32,      // 成功时固定 200
    pub msg: String,    // 提示消息
    pub data: T,        // 业务数据
}
```

### 构建方法

| 方法 | 用途 | 输出 |
|------|------|------|
| `ApiResponse::ok(data)` | 成功，带数据 | `{"code": 200, "msg": "", "data": ...}` |
| `ApiResponse::ok_with_msg(data, "提示")` | 成功，带数据+提示 | `{"code": 200, "msg": "提示", "data": ...}` |
| `ApiResponse::warn(1001, data, "警告")` | 业务警告 | `{"code": 1001, "msg": "警告", "data": ...}` |
| `ApiResponse::warn_code(1001, data)` | 业务警告，无消息 | `{"code": 1001, "msg": "", "data": ...}` |
| `ApiResponse::empty()` | 成功，无数据 | `{"code": 200, "msg": "", "data": null}` |
| `ApiResponse::empty_with_msg("提示")` | 成功，无数据+提示 | `{"code": 200, "msg": "提示", "data": null}` |

### 成功 vs 错误的职责划分

| HTTP 状态 | 处理方 | 格式 |
|-----------|--------|------|
| 2xx | `ApiResponse` | `{"code": 200, "msg": "...", "data": ...}` |
| 4xx / 5xx | `ApiErrors` → ProblemDetails | `{"type", "title", "status", "detail", "instance"}` |

两者被 HTTP 状态码天然隔开，前端按 HTTP 状态码区分解析逻辑（详见 `doc/frontend-adaptation-guide.md`）。

---

## 分页规范

### PageQuery（common/src/request.rs）

分页请求参数，接收前端的 `current`（1 起始）和 `size`，自动转换为 spring-sea-orm 的 `Pagination`（0 起始）：

```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PageQuery {
    pub current: u64,   // 当前页码，从 1 开始（默认 1）
    pub size: u64,      // 每页条数（默认 10）
}

// 自动转换：PageQuery { current: 1, size: 10 } → Pagination { page: 0, size: 10 }
impl From<PageQuery> for Pagination { ... }
```

### PageResponse（common/src/response.rs）

分页响应，配合 `ApiResponse` 使用：

```rust
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub records: Vec<T>,
    pub current: u64,
    pub size: u64,
    pub total: u64,
}

// 自动从 spring-sea-orm 的 Page 转换，page 自动从 0 起始转为 1 起始
impl<T: Serialize> From<Page<T>> for PageResponse<T> { ... }
```

### 分页接口示例

```rust
#[get("/api/user/list")]
pub async fn list(
    Query(query): Query<PageQuery>,
    Component(db): Component<DbConn>,
) -> ApiResult<ApiResponse<PageResponse<UserVo>>> {
    let pagination: Pagination = query.into();
    let page = SysUser::find()
        .page(db.as_ref(), &pagination)
        .await
        .map_err(|e| ApiErrors::Internal(e.into()))?;
    Ok(ApiResponse::ok(PageResponse::from(page.map(UserVo::from))))
}
```

完整数据流：

```
前端 ?current=1&size=10
  → PageQuery { current: 1, size: 10 }
  → Pagination { page: 0, size: 10 }
  → spring-sea-orm 查询
  → Page { page: 0, content: [...], total_elements: 56, ... }
  → PageResponse { current: 1, records: [...], size: 10, total: 56 }
  → {"code": 200, "msg": "", "data": {"records": [...], "current": 1, "size": 10, "total": 56}}
```

---

## Router 规范

### 文件组织

每个资源一个文件，放在 `app/src/router/` 下：

```
router/
├── mod.rs           # pub mod sys_user; pub mod sys_role; ...
├── sys_user.rs
├── sys_role.rs
└── sys_menu.rs
```

### 返回值类型

所有 handler 统一返回 `ApiResult<ApiResponse<T>>`：

```rust
use common::error::{ApiErrors, ApiResult};
use common::response::ApiResponse;
```

### 简单 CRUD — 直接写

单表查询、简单增删改，直接在 router 里操作 ORM，不经过 service：

```rust
#[get("/api/user/{id}")]
pub async fn get_by_id(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<UserVo>> {
    let user = SysUser::find_by_id(id)
        .one(&db)
        .await
        .context("查询用户失败")?
        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;
    Ok(ApiResponse::ok(user.into()))
}

#[delete("/api/user/{id}")]
pub async fn delete(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    SysUser::delete_by_id(id)
        .exec(&db)
        .await
        .context("删除用户失败")?;
    Ok(ApiResponse::empty())
}
```

### 复杂业务 — 委托 Service

涉及多表操作、事务、业务校验、可复用逻辑时，委托 service：

```rust
#[post("/api/user")]
pub async fn create(
    Component(svc): Component<SysUserService>,
    Json(dto): Json<CreateUserDto>,
) -> ApiResult<ApiResponse<UserVo>> {
    let user = svc.create_user(dto).await?;
    Ok(ApiResponse::ok(user.into()))
}
```

### 判断标准

| 场景 | 放哪里 |
|------|--------|
| 单表查询、列表、分页 | router 直接写 |
| 单表简单插入、删除 | router 直接写 |
| 需要查重、校验再写入 | service |
| 多表操作、事务 | service |
| 多个路由复用同一段逻辑 | service |

---

## Service 规范

### 结构

使用 `#[derive(Service)]` + `#[inject(component)]` 实现编译期依赖注入：

```rust
#[derive(Clone, Service)]
pub struct SysUserService {
    #[inject(component)]
    db: DbConn,
}
```

### 返回值类型

Service 方法返回 `ApiResult<T>`，直接使用 `ApiErrors` 变体表达业务错误：

```rust
impl SysUserService {
    pub async fn create_user(&self, dto: CreateUserDto) -> ApiResult<sys_user::Model> {
        // 业务校验 — 直接返回 ApiErrors 变体
        let existing = SysUser::find()
            .filter(sys_user::Column::Username.eq(&dto.username))
            .one(&self.db)
            .await
            .context("检查用户名失败")?;  // anyhow → ApiErrors::Internal (500)

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!("用户名已存在: {}", dto.username)));
        }

        // 数据操作
        let user = sys_user::ActiveModel { ... };
        let user = user.insert(&self.db).await.context("创建用户失败")?;
        Ok(user)
    }
}
```

### 错误传播机制

- `.context("...")?` — 基础设施错误通过 `anyhow` 链式传播，自动转为 `ApiErrors::Internal`（500）
- `Err(ApiErrors::Xxx(...))` — 业务错误直接返回对应变体，携带正确的 HTTP 状态码
- Router 中 `?` 直接传播，无需 `map_err`

### 注意事项

- Service 操作的是 entity Model，不是 VO；Model → VO 转换交给 router

---

## Model 规范

### Entity（实体）

对应数据库表，由 SeaORM 定义：

```rust
// model/src/entity/sys_user.rs
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub username: String,
    pub password: String,
    // ...
}
```

### DTO（请求参数）

接收前端请求，只需 `Deserialize`：

```rust
#[derive(Debug, Deserialize)]
pub struct CreateUserDto {
    pub username: String,
    pub password: String,
    pub nickname: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}
```

命名规则：`{动作}{资源}Dto`，如 `CreateUserDto`、`UpdateUserDto`、`ResetPasswordDto`。

### VO（响应对象）

返回给前端，需要 `Serialize` + `JsonSchema`，**不包含敏感字段**（如 password）：

```rust
#[derive(Debug, Serialize, JsonSchema)]
pub struct UserVo {
    pub id: i64,
    pub username: String,
    pub nickname: String,
    pub email: Option<String>,
    pub status: i16,
    pub created_at: NaiveDateTime,
}

impl From<entity::sys_user::Model> for UserVo {
    fn from(m: entity::sys_user::Model) -> Self {
        // 映射字段，过滤 password
    }
}
```

命名规则：`{资源}Vo`，如 `UserVo`、`RoleVo`。

### DTO vs VO 不是必须的

简单场景下可以不建 DTO/VO：
- 如果请求参数就是 entity 本身的子集，可以直接用 entity
- 如果响应就是 entity 本身（无敏感字段），可以直接返回 Model

但以下场景**必须**拆分：
- 响应中需要过滤敏感字段（password、secret 等）→ 用 VO
- 请求参数和 entity 字段差异大 → 用 DTO
- 需要对请求参数做校验（validator）→ 用 DTO

---

## 新增一个接口的完整流程

以「新增角色」为例：

### 1. 建表（sql/）

```sql
CREATE TABLE sys_role (
    id          BIGSERIAL   PRIMARY KEY,
    name        VARCHAR(64) NOT NULL UNIQUE,
    code        VARCHAR(64) NOT NULL UNIQUE,
    status      SMALLINT    NOT NULL DEFAULT 1,
    created_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### 2. 定义 Entity（model/entity/）

```rust
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_role")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub code: String,
    pub status: i16,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}
```

### 3. 定义 DTO（model/dto/）

```rust
#[derive(Debug, Deserialize)]
pub struct CreateRoleDto {
    pub name: String,
    pub code: String,
}
```

### 4. 定义 VO（model/vo/）

```rust
#[derive(Debug, Serialize, JsonSchema)]
pub struct RoleVo {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub status: i16,
    pub created_at: NaiveDateTime,
}

impl From<entity::sys_role::Model> for RoleVo { ... }
```

### 5. 编写 Router（app/router/）

```rust
// 角色列表（分页查询）
#[get("/api/role/list")]
pub async fn list(
    Query(query): Query<PageQuery>,
    Component(db): Component<DbConn>,
) -> ApiResult<ApiResponse<PageResponse<RoleVo>>> {
    let pagination: Pagination = query.into();
    let page = SysRole::find()
        .page(db.as_ref(), &pagination)
        .await
        .map_err(|e| ApiErrors::Internal(e.into()))?;
    Ok(ApiResponse::ok(PageResponse::from(page.map(RoleVo::from))))
}

// 创建角色
#[post("/api/role")]
pub async fn create(
    Component(db): Component<DbConn>,
    Json(dto): Json<CreateRoleDto>,
) -> ApiResult<ApiResponse<RoleVo>> {
    let role = sys_role::ActiveModel {
        name: Set(dto.name),
        code: Set(dto.code),
        ..Default::default()
    };
    let role = role.insert(&db).await.context("创建角色失败")?;
    Ok(ApiResponse::ok(role.into()))
}
```

### 6. 注册模块

在对应的 `mod.rs` 中添加 `pub mod sys_role;`。

---

## 总结

| 层 | 职责 | 返回值 | 依赖 |
|----|------|--------|------|
| **router** | 接收请求、参数提取、调用 ORM 或 Service、返回响应 | `ApiResult<ApiResponse<T>>` | model, common, spring-web |
| **service** | 复杂业务逻辑、事务、多表操作、可复用逻辑 | `ApiResult<T>` | model, common, sea-orm, anyhow |
| **model** | 数据结构定义（entity + dto + vo） | — | sea-orm, serde |
| **common** | 统一错误（ApiErrors）、统一请求（PageQuery）、统一响应（ApiResponse / PageResponse）| — | spring-web, spring-sea-orm, thiserror, anyhow, serde |