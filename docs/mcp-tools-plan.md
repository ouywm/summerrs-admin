# MCP Tools 功能规划

> 基于 summerrs-admin 后台管理系统的 MCP Server 工具规划
> 目标：通过 MCP 协议让 AI Agent（Claude Code / Cursor）直接操作项目，加速开发

---

## 一、工具分类总览

| 分类 | 工具数 | 核心价值 |
|------|--------|----------|
| A. 代码生成 | 6 | 从数据库表到完整 CRUD 全链路生成 |
| B. 数据库操作 | 5 | AI 直接查询/修改数据库 |
| C. 菜单数据管理 | 3 | 前端路由 → 菜单表增量同步 |
| D. 宏辅助 | 3 | 一键给接口添加 log/权限/角色宏 |
| E. 项目分析 | 4 | 架构理解、代码导航 |
| F. 前端联动 | 3 | 后端结构体 → 前端 TS 类型/API/页面 |

---

## A. 代码生成工具

### A1. `gen_entity` — Sea-ORM 实体生成

**描述**: 连接数据库，对指定表生成 Sea-ORM entity 文件

**参数**:
- `table_name: String` — 数据库表名（如 `sys_department`）
- `overwrite: bool` — 是否覆盖已有文件（默认 false）

**行为**:
1. 调用 `sea-orm-cli generate entity` 或直接查询 `information_schema`
2. 生成 `crates/model/src/entity/{table_name}.rs`
3. 自动在 `entity/mod.rs` 中追加 `pub mod {table_name};`
4. 按项目现有模式添加 `ActiveModelBehavior`（自动时间戳）
5. 识别枚举字段，生成对应 `#[derive(EnumIter, DeriveActiveEnum)]` 枚举

**输出**: 生成的文件路径和实体结构摘要

---

### A2. `gen_dto` — DTO 生成

**描述**: 根据 entity 生成对应的 DTO（CreateDto / UpdateDto / QueryDto）

**参数**:
- `entity_name: String` — 实体名（如 `sys_department`）
- `operations: Vec<String>` — 要生成的操作（`create` / `update` / `query` / `all`）

**行为**:
1. 读取 `entity/{entity_name}.rs`，解析所有字段
2. 按项目模式生成 DTO：
   - `CreateDto` — 排除 `id`、`create_time`、`update_time`、`create_by`、`update_by`
   - `UpdateDto` — 所有字段可选（`Option<T>`），保留 `id`
   - `QueryDto` — 时间范围字段（`begin_time` / `end_time`）、模糊查询字段、枚举筛选字段
3. 添加 `#[derive(Deserialize, Validate)]` 和字段校验注解
4. 写入 `crates/model/src/dto/{entity_name}.rs`
5. 自动在 `dto/mod.rs` 追加模块声明

**输出**: 生成的 DTO 文件路径和字段清单

---

### A3. `gen_vo` — VO 生成

**描述**: 根据 entity 生成对应的 VO（响应对象）

**参数**:
- `entity_name: String` — 实体名
- `types: Vec<String>` — 要生成的类型（`list` / `detail` / `simple` / `all`）

**行为**:
1. 读取 entity 字段
2. 生成 VO：
   - `{Name}Vo` — 列表页用，排除敏感字段（password）
   - `{Name}DetailVo` — 详情页用，包含关联数据
   - `{Name}SimpleVo` — 下拉选择用，仅 `id` + `name`
3. 添加 `#[derive(Serialize, ToSchema)]`
4. 写入 `crates/model/src/vo/{entity_name}.rs`

---

### A4. `gen_service` — Service 生成

**描述**: 根据 entity/dto/vo 生成 Service 层代码

**参数**:
- `entity_name: String` — 实体名
- `operations: Vec<String>` — CRUD 操作（`create` / `update` / `delete` / `list` / `detail` / `all`）

**行为**:
1. 读取 entity、dto、vo 文件
2. 按项目模式生成 service：
   - `create_{name}(dto, operator)` — 从 DTO 构建 ActiveModel 并 insert
   - `update_{name}(id, dto, operator)` — 部分更新
   - `delete_{name}(id)` — 删除（物理/逻辑）
   - `list_{name}(query, pagination)` — 分页查询，支持筛选
   - `get_{name}(id)` — 详情查询
3. 使用 `DatabaseConnection` 注入
4. 分页使用项目自定义 `Pagination` 组件
5. 写入 `crates/app/src/service/{entity_name}_service.rs`
6. 自动在 `service/mod.rs` 追加模块声明

---

### A5. `gen_router` — Router 生成

**描述**: 根据 service 生成路由 handler 代码

**参数**:
- `entity_name: String` — 实体名
- `module_name: String` — 模块中文名（用于 `#[log]` 宏的 `module` 参数）
- `perm_prefix: String` — 权限前缀（如 `system:dept`）
- `operations: Vec<String>` — CRUD 操作

**行为**:
1. 读取 service 文件，获取方法签名
2. 为每个操作生成 handler：
   ```rust
   #[log(module = "{module_name}", action = "创建{name}", biz_type = Create)]
   #[has_perm("{perm_prefix}:add")]
   #[post_api("/{path}")]
   pub async fn create_{name}(...) -> ApiResult<()> { ... }
   ```
3. 自动选择正确的宏和提取器
4. 写入 `crates/app/src/router/{entity_name}.rs`
5. 在 `router/mod.rs` 和 `main.rs` 中注册路由

---

### A6. `gen_crud` — 全链路一键生成

**描述**: 一键生成 Entity → DTO → VO → Service → Router 全链路代码

**参数**:
- `table_name: String` — 数据库表名
- `module_name: String` — 模块中文名
- `perm_prefix: String` — 权限前缀
- `operations: Vec<String>` — 要生成的操作（默认 `all`）

**行为**: 依次调用 A1 → A2 → A3 → A4 → A5

**输出**: 所有生成文件的清单和模块注册状态

---

## B. 数据库操作工具

### B1. `db_query` — 执行 SQL 查询

**描述**: 在数据库上执行只读 SQL 查询

**参数**:
- `sql: String` — SQL 语句（仅允许 SELECT）
- `limit: Option<u32>` — 结果限制（默认 100，防止大量数据返回）

**行为**:
1. 校验 SQL 为只读操作（禁止 INSERT/UPDATE/DELETE/DROP/ALTER/TRUNCATE）
2. 强制添加 LIMIT（如果没有）
3. 通过 Sea-ORM `DatabaseConnection` 执行
4. 结果格式化为表格文本返回

**安全**: 只读，防注入校验

**示例**: "显示数据库中的所有用户" → `SELECT id, user_name, nick_name, status FROM sys_user LIMIT 100`

---

### B2. `db_execute` — 执行 SQL 修改

**描述**: 执行写入操作（INSERT/UPDATE/DELETE）

**参数**:
- `sql: String` — SQL 语句
- `confirm: bool` — 用户确认（通过 Elicitation 二次确认）

**行为**:
1. 解析 SQL 类型，展示影响范围预估
2. 通过 Elicitation 请求用户确认
3. 在事务中执行
4. 返回影响行数

**安全**: 禁止 DROP/ALTER/TRUNCATE，UPDATE/DELETE 必须带 WHERE

**示例**: "添加一个名为张三的新用户" → 构造并执行 INSERT 语句

---

### B3. `db_schema` — 查询数据库架构

**描述**: 查询数据库表结构信息

**参数**:
- `target: String` — `tables`（所有表）| `{table_name}`（指定表的列信息）| `relations`（表关联）

**行为**:
1. `tables` — 查询 `information_schema.tables`，返回表名、注释、行数估算
2. `{table_name}` — 查询列名、类型、是否可空、默认值、注释
3. `relations` — 查询外键关系

**输出**: 格式化的表结构信息

---

### B4. `db_status` — 数据库连接状态

**描述**: 返回数据库连接池状态

**参数**: 无

**行为**: 查询连接池指标（活跃连接、空闲连接、等待数、最大连接数）

---

### B5. `cache_query` — Redis 缓存操作

**描述**: 查询或操作 Redis 缓存

**参数**:
- `operation: String` — `keys`（列出键）| `get`（获取值）| `delete`（删除键）
- `pattern: Option<String>` — 键名模式（如 `auth:session:*`）
- `key: Option<String>` — 具体键名

**行为**: 通过 Redis 连接执行对应操作

---

## C. 菜单数据管理工具

### C1. `menu_sync_from_frontend` — 前端路由同步到菜单表

**描述**: 读取前端路由文件，增量生成菜单 SQL 插入语句

**参数**:
- `frontend_path: String` — 前端项目根路径
- `router_dir: Option<String>` — 路由文件目录（默认 `src/router`）
- `dry_run: bool` — 是否仅预览不执行（默认 true）

**行为**:
1. 扫描前端项目的路由配置文件（如 `src/router/modules/*.ts`）
2. 解析路由树：`path`、`name`、`component`、`meta`（title、icon、hidden 等）
3. 查询现有 `sys_menu` 表数据
4. 对比差异，仅生成增量：
   - 新路由 → `INSERT INTO sys_menu (...)`
   - 已存在 → 跳过
5. 自动确定 `parent_id`（根据路由嵌套关系）
6. 自动计算 `sort` 排序值
7. 区分 `menu_type`：路由 → Menu(1)，权限按钮 → Button(2)
8. 若 `dry_run = false`，通过 Elicitation 确认后执行

**输出**: 增量 SQL 语句列表 + 变更摘要

---

### C2. `menu_tree` — 查看菜单树

**描述**: 以树形结构展示当前菜单表数据

**参数**:
- `include_buttons: bool` — 是否包含按钮权限（默认 true）
- `role_code: Option<String>` — 按角色筛选可见菜单

**行为**: 查询 `sys_menu` 表，构建树形结构，格式化输出

---

### C3. `menu_add` — 添加菜单/按钮

**描述**: 交互式添加菜单项或按钮权限

**参数**:
- `parent_path: Option<String>` — 父级菜单路径（用于定位 parent_id）
- `menu_type: String` — `menu` | `button`
- `title: String` — 菜单标题
- `path: Option<String>` — 路由路径
- `component: Option<String>` — 组件路径
- `auth_mark: Option<String>` — 权限标识（按钮必填）
- `icon: Option<String>` — 图标
- `sort: Option<i32>` — 排序

**行为**:
1. 根据 `parent_path` 查找父菜单 ID
2. 若为按钮，自动分配 `bit_position`
3. 构造 INSERT 语句并执行
4. 返回新增记录

---

## D. 宏辅助工具

### D1. `add_log_macro` — 给接口添加 #[log] 宏

**描述**: 扫描指定路由文件，为缺少 `#[log]` 宏的 handler 函数自动添加

**参数**:
- `file_path: String` — 路由文件路径（如 `crates/app/src/router/sys_user.rs`）
- `module_name: String` — 模块名（如 "用户管理"）
- `dry_run: bool` — 仅预览不修改（默认 true）

**行为**:
1. 解析文件中所有 handler 函数（识别 `#[get_api]` / `#[post_api]` / `#[put_api]` / `#[delete_api]`）
2. 检查每个 handler 是否已有 `#[log]`
3. 为缺少的 handler 生成 `#[log]` 宏：
   - 根据 HTTP 方法推断 `biz_type`：POST → Create，PUT → Update，DELETE → Delete，GET → Query
   - 根据函数名推断 `action`：`create_user` → "创建用户"
   - 登录相关接口自动设置 `save_params = false`
4. 返回差异预览或直接修改文件

**输出**: 修改前后的 diff

---

### D2. `add_perm_macro` — 给接口添加权限宏

**描述**: 扫描路由文件，为缺少权限检查的 handler 添加 `#[has_perm]` 宏

**参数**:
- `file_path: String` — 路由文件路径
- `perm_prefix: String` — 权限前缀（如 `system:user`）
- `dry_run: bool` — 仅预览（默认 true）

**行为**:
1. 解析 handler 函数
2. 检查是否已有 `#[has_perm]` / `#[has_perms]` / `#[has_role]` / `#[login]`
3. 根据函数名和 HTTP 方法推断权限后缀：
   - `list_*` / `get_*` → `{prefix}:list`
   - `create_*` / `add_*` → `{prefix}:add`
   - `update_*` / `edit_*` → `{prefix}:edit`
   - `delete_*` / `remove_*` → `{prefix}:delete`
   - `export_*` → `{prefix}:export`
4. 在 `#[log]` 和 HTTP 方法宏之间插入权限宏

---

### D3. `add_macros_batch` — 批量添加宏

**描述**: 对整个 router 目录批量添加 log + 权限宏

**参数**:
- `router_dir: Option<String>` — 路由目录（默认 `crates/app/src/router/`）
- `dry_run: bool` — 仅预览（默认 true）

**行为**:
1. 扫描目录下所有 `.rs` 文件
2. 根据文件名推断模块名和权限前缀
3. 逐文件调用 D1 + D2
4. 汇总报告

---

## E. 项目分析工具

### E1. `project_structure` — 项目结构概览

**描述**: 返回项目的模块结构、路由清单、服务清单

**参数**:
- `scope: String` — `overview`（总览）| `routes`（路由）| `services`（服务）| `entities`（实体）| `macros`（宏）

**行为**: 读取对应目录，生成结构化摘要

---

### E2. `api_list` — API 接口清单

**描述**: 列出所有已注册的 API 接口

**参数**:
- `module: Option<String>` — 按模块筛选
- `method: Option<String>` — 按 HTTP 方法筛选

**行为**: 解析所有 router 文件，提取 `#[get_api]` / `#[post_api]` 等宏中的路径

**输出**: 接口列表（方法、路径、函数名、是否有 log 宏、是否有权限宏）

---

### E3. `entity_relations` — 实体关联图

**描述**: 展示 entity 之间的关联关系

**参数**: 无

**行为**: 解析所有 entity 文件中的 `Related<T>` 实现，生成关联关系图

---

### E4. `perm_tree` — 权限树

**描述**: 展示完整的权限树（角色 → 菜单/按钮权限）

**参数**:
- `role_code: Option<String>` — 按角色筛选

**行为**: 查询 sys_role → sys_role_menu → sys_menu，构建权限树

---

## F. 前端联动工具

### F1. `gen_ts_types` — 生成 TypeScript 类型

**描述**: 根据后端 DTO/VO 结构体生成对应的 TypeScript 类型定义

**参数**:
- `entity_name: String` — 实体名
- `output_dir: Option<String>` — 输出目录（默认 `{frontend}/src/api/types/`）

**行为**:
1. 读取 DTO 和 VO 文件
2. Rust 类型 → TypeScript 类型映射：
   - `String` → `string`
   - `i64` / `i32` → `number`
   - `bool` → `boolean`
   - `Option<T>` → `T | null`
   - `DateTime` → `string`（ISO 8601）
   - `Vec<T>` → `T[]`
   - `enum` → TypeScript union type
3. 生成 `.ts` 文件

**输出示例**:
```typescript
// sys_user.ts
export interface CreateUserDto {
  user_name: string;
  password: string;
  nick_name: string;
  gender?: number;
  phone?: string;
  email?: string;
}

export interface UserVo {
  id: number;
  user_name: string;
  nick_name: string;
  gender: number;
  status: number;
  create_time: string;
}
```

---

### F2. `gen_ts_api` — 生成 TypeScript API 调用

**描述**: 根据后端路由生成前端 API 请求函数

**参数**:
- `entity_name: String` — 实体名
- `output_dir: Option<String>` — 输出目录（默认 `{frontend}/src/api/`）

**行为**:
1. 读取 router 文件，提取所有 API 端点
2. 生成对应的 API 调用函数（使用 axios/fetch）

**输出示例**:
```typescript
// sys_user.ts
import request from '@/utils/request';
import type { CreateUserDto, UpdateUserDto, UserQueryDto, UserVo, UserDetailVo } from './types/sys_user';
import type { PageResult } from './types/common';

export function listUsers(params: UserQueryDto) {
  return request.get<PageResult<UserVo>>('/api/user/list', { params });
}
export function createUser(data: CreateUserDto) {
  return request.post('/api/user', data);
}
export function updateUser(id: number, data: UpdateUserDto) {
  return request.put(`/api/user/${id}`, data);
}
export function deleteUser(id: number) {
  return request.delete(`/api/user/${id}`);
}
```

---

### F3. `gen_vue_page` — 生成 Vue 页面（骨架）

**描述**: 根据 entity/vo 结构生成 Vue 页面骨架代码

**参数**:
- `entity_name: String` — 实体名
- `page_type: String` — `list`（列表页）| `form`（表单页）| `detail`（详情页）| `all`
- `output_dir: Option<String>` — 输出目录

**行为**:
1. 读取 VO 字段信息
2. 按字段类型生成对应的 UI 组件：
   - `String` → Input
   - `i32/i64` → InputNumber
   - `bool` → Switch
   - `enum` → Select（关联字典）
   - `DateTime` → DatePicker
   - `Option<T>` → 非必填
3. 列表页：Table + 搜索表单 + 分页
4. 表单页：Form + 校验规则
5. 详情页：Descriptions 描述列表

---

## 实现优先级

### Phase 1 — 核心基础（数据库感知）
> MCP 能"看到"和"理解"数据库

| 工具 | 依赖 |
|------|------|
| B3. `db_schema` | DatabaseConnection |
| B4. `db_status` | DatabaseConnection |
| B1. `db_query` | DatabaseConnection |
| E1. `project_structure` | 文件系统读取 |
| C2. `menu_tree` | DatabaseConnection |
| E4. `perm_tree` | DatabaseConnection |

### Phase 2 — 代码生成（开发提效）
> MCP 能"生成"规范代码

| 工具 | 依赖 |
|------|------|
| A1. `gen_entity` | sea-orm-cli 或 information_schema |
| A2. `gen_dto` | entity 文件解析 |
| A3. `gen_vo` | entity 文件解析 |
| A4. `gen_service` | entity + dto + vo |
| A5. `gen_router` | service |
| A6. `gen_crud` | A1-A5 |

### Phase 3 — 数据操作 + 宏辅助
> MCP 能"修改"数据和代码

| 工具 | 依赖 |
|------|------|
| B2. `db_execute` | DatabaseConnection + Elicitation |
| B5. `cache_query` | Redis 连接 |
| D1. `add_log_macro` | 文件解析 + 写入 |
| D2. `add_perm_macro` | 文件解析 + 写入 |
| D3. `add_macros_batch` | D1 + D2 |
| E2. `api_list` | 文件解析 |

### Phase 4 — 前端联动 + 菜单同步
> MCP 打通前后端全链路

| 工具 | 依赖 |
|------|------|
| C1. `menu_sync_from_frontend` | 前端路由解析 + DB |
| C3. `menu_add` | DatabaseConnection |
| F1. `gen_ts_types` | DTO/VO 解析 |
| F2. `gen_ts_api` | Router 解析 |
| F3. `gen_vue_page` | VO 解析 |
| E3. `entity_relations` | Entity 解析 |

---

## 架构依赖

MCP 工具需要注入的核心依赖：

```
AdminMcpServer
├── DatabaseConnection      ← Phase 1 必需（sea-orm）
├── RedisConnection         ← Phase 3 必需（cache_query）
├── ProjectRootPath         ← 文件系统操作
└── FrontendProjectPath     ← Phase 4 前端联动（可选）
```

这意味着 `AdminMcpServer::new()` 需要扩展为接收这些依赖，
`McpPlugin::build()` 中从 `AppBuilder` 获取已注册的组件注入到 MCP Server。

---

## Prompt 模板规划

除工具外，还应提供以下 Prompt 模板：

| Prompt | 参数 | 用途 |
|--------|------|------|
| `crud_guide` | `table_name` | 生成 CRUD 开发指南（按项目规范） |
| `api_design` | `module_name`, `operations` | 生成 RESTful API 设计建议 |
| `code_review` | `file_path` | 按项目规范审查代码 |
| `migration_sql` | `table_name`, `columns` | 生成建表/改表 SQL |

---

## Resource 规划

动态暴露项目关键信息为 MCP Resource：

| Resource URI | 内容 |
|-------------|------|
| `project://structure` | 项目目录结构 |
| `db://tables` | 数据库表清单 |
| `db://table/{name}` | 指定表的列信息 |
| `api://routes` | API 路由清单 |
| `perm://tree` | 权限树 |
| `entity://relations` | 实体关联图 |
