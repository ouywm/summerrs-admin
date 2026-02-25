# summerrs-admin 开发规范

## 项目结构

```
summerrs-admin/
├── Cargo.toml                     # workspace 根，集中管理所有依赖版本
├── config/                        # 应用配置文件
│   ├── app.toml
│   ├── app-dev.toml
│   ├── app-prod.toml
│   └── app-test.toml
├── sql/                           # 数据库脚本
│   └── init.sql
├── doc/                           # 项目文档
├── crates/
│   ├── app/                       # bin crate - 程序入口 + 路由
│   │   └── src/
│   │       ├── main.rs            # App 启动、插件注册
│   │       └── router/            # 路由处理器（按资源分文件）
│   │           ├── mod.rs
│   │           └── sys_user.rs
│   ├── model/                     # lib crate - 数据模型
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── entity/            # SeaORM 实体（后续由 cli 生成到 _entities/）
│   │       ├── dto/               # 请求参数
│   │       ├── vo/                # 响应对象
│   │       └── views/             # 预留，复合视图类型
│   ├── service/                   # lib crate - 复杂业务逻辑
│   │   └── src/
│   │       ├── lib.rs
│   │       └── sys_user.rs
│   └── common/                    # lib crate - 公共工具
│       └── src/
│           └── lib.rs
```

## 依赖方向

```
app → service → model → common
app → model  (router 直接用 entity/dto/vo)
app → common
```

单向流动，不允许反向依赖。

---

## Router 规范

### 文件组织

每个资源一个文件，放在 `app/src/router/` 下：

```
router/
├── mod.rs           # pub mod sys_user; pub mod sys_role; ...
├── sys_user.rs      # 用户相关路由
├── sys_role.rs      # 角色相关路由
└── sys_menu.rs      # 菜单相关路由
```

### 简单 CRUD — 直接写

单表查询、简单增删改，直接在 router 里操作 ORM，不经过 service：

```rust
use spring_sea_orm::DbConn;
use spring_web::extractor::Component;

/// 获取用户列表
#[get("/api/sys-user/list")]
pub async fn list(Component(db): Component<DbConn>) -> Result<Json<Vec<UserVo>>> {
    let users = SysUser::find()
        .all(&db)
        .await
        .context("查询用户列表失败")?;
    Ok(Json(users.into_iter().map(UserVo::from).collect()))
}

/// 删除用户
#[delete("/api/sys-user/{id}")]
pub async fn delete(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> Result<Json<bool>> {
    SysUser::delete_by_id(id)
        .exec(&db)
        .await
        .context("删除用户失败")?;
    Ok(Json(true))
}
```

### 复杂业务 — 委托 Service

涉及多表操作、事务、业务校验、可复用逻辑时，委托 service：

```rust
use service::sys_user::SysUserService;

/// 创建用户（含查重、密码加密等）
#[post("/api/sys-user")]
pub async fn create(
    Component(svc): Component<SysUserService>,
    Json(dto): Json<CreateUserDto>,
) -> Result<Json<UserVo>> {
    let user = svc.create_user(dto).await?;
    Ok(Json(user.into()))
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
use spring::plugin::service::Service;
use spring_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct SysUserService {
    #[inject(component)]
    db: DbConn,
}

impl SysUserService {
    pub async fn create_user(&self, dto: CreateUserDto) -> anyhow::Result<sys_user::Model> {
        // 业务校验
        // 数据操作
        // 返回结果
    }
}
```

### 注意事项

- Service 方法返回 `anyhow::Result<T>`，由 router 的 `Result` 自动转换为 HTTP 错误
- Service 不依赖 spring-web，不感知 HTTP 概念（不返回 Json、不抛 KnownWebError）
- Service 操作的是 entity Model，不是 VO；转换交给 router

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

后续接入数据库后，使用 `sea-orm-cli generate entity` 生成到 `_entities/` 目录，外层文件 re-export 并扩展。

### DTO（请求参数）

接收前端请求，只需 `Deserialize`：

```rust
// model/src/dto/sys_user.rs
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

返回给前端，只需 `Serialize`，**不包含敏感字段**（如 password）：

```rust
// model/src/vo/sys_user.rs
#[derive(Debug, Serialize)]
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

## 返回值规范

### 成功

直接返回数据，不做额外包装：

```rust
// 单个对象
-> Result<Json<UserVo>>

// 列表
-> Result<Json<Vec<UserVo>>>

// 分页（使用 spring-sea-orm 内置）
-> Result<Json<Page<UserVo>>>

// 操作结果
-> Result<Json<bool>>
```

### 错误

使用 `spring_web::error` 提供的错误类型，框架自动转换为 HTTP 响应：

```rust
use spring_web::error::{KnownWebError, Result};

// 404
.ok_or_else(|| KnownWebError::not_found("用户不存在"))?;

// 400
return Err(KnownWebError::bad_request("用户名已存在"))?;

// 500（通过 anyhow 自动转换）
.context("数据库查询失败")?;
```

**不使用自定义响应包装类型**（如 `R<T>`），HTTP 协议本身区分成功和失败。

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
// model/src/entity/sys_role.rs
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
// model/src/dto/sys_role.rs
#[derive(Debug, Deserialize)]
pub struct CreateRoleDto {
    pub name: String,
    pub code: String,
}
```

### 4. 定义 VO（model/vo/）

```rust
// model/src/vo/sys_role.rs
#[derive(Debug, Serialize)]
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
// app/src/router/sys_role.rs

/// 角色列表（简单查询，直接写）
#[get("/api/sys-role/list")]
pub async fn list(Component(db): Component<DbConn>) -> Result<Json<Vec<RoleVo>>> {
    let roles = SysRole::find().all(&db).await.context("查询角色列表失败")?;
    Ok(Json(roles.into_iter().map(RoleVo::from).collect()))
}

/// 创建角色（需要查重，走 service 或直接写都可以，视复杂度而定）
#[post("/api/sys-role")]
pub async fn create(
    Component(db): Component<DbConn>,
    Json(dto): Json<CreateRoleDto>,
) -> Result<Json<RoleVo>> {
    let role = sys_role::ActiveModel {
        name: Set(dto.name),
        code: Set(dto.code),
        ..Default::default()
    };
    let role = role.insert(&db).await.context("创建角色失败")?;
    Ok(Json(role.into()))
}
```

### 6. 注册模块

在对应的 `mod.rs` 中添加 `pub mod sys_role;`。

---

## 总结

| 层 | 职责 | 依赖 |
|----|------|------|
| **router** | 接收请求、参数提取、调用 ORM 或 Service、返回响应 | model, service, spring-web |
| **service** | 复杂业务逻辑、事务、多表操作、可复用逻辑 | model, sea-orm, anyhow |
| **model** | 数据结构定义（entity + dto + vo） | sea-orm, serde |
| **common** | 公共工具、常量 | 基础库 |
