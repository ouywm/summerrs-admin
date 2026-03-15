# 后端接口开发需求清单

> 本文档梳理了前端所有页面所需的后端接口，按**优先级**排列。
> 后端开发请优先实现 P0 级接口，确保核心流程跑通后，再按优先级逐步实现其他接口。
>
> **前置文档：** 请先阅读 `doc/backend-api-guide.md`（响应格式、Token 机制、分页规范等基础约定）
> 和 `doc/frontend-adaptation-guide.md`（错误响应 ProblemDetails 格式说明）。

> **范围说明（2026-03-13）：** 本期不开发：部门/岗位、文章/分类、留言墙/评论。

---

## 接口总览

| 优先级 | 模块 | 接口数量 | 说明 |
|:------:|------|:--------:|------|
| **P0** | 认证 | 3 | 登录、注册、获取用户信息 |
| **P0** | 菜单 | 1 | 获取菜单列表（后端权限模式必须） |
| **P1** | 用户管理 | 3 | 用户列表、新增、编辑、删除 |
| **P1** | 角色管理 | 5 | 角色 CRUD + 权限分配 |
| **P1** | 菜单管理 | 3 | 菜单 CRUD（后端权限模式） |
| **P2** | 系统参数配置 | 6 | 参数 CRUD + 按 key 获取 + 刷新缓存（可选） |
| **P3** | 定时任务 | 5 | 任务列表/启停/手动触发 + 运行记录 |

---

## P0 — 核心认证（最高优先级）

> 没有这些接口，前端无法登录和使用。已在 `backend-api-guide.md` 中详细说明。

### 1. 用户登录

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/auth/login` |
| 需要 Token | 否 |
| 前端源码 | `src/api/auth.ts` → `fetchLogin()` |
| 状态 | **已实现** |

**请求参数：**

```json
{
  "userName": "admin",
  "password": "123456"
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "登录成功",
  "data": {
    "token": "eyJhbGciOiJIUzI1NiJ9...",
    "refreshToken": "eyJhbGciOiJIUzI1NiJ9..."
  }
}
```

> 详细说明见 `backend-api-guide.md` 5.1 节。

---

### 2. 获取用户信息

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/user/info` |
| 需要 Token | **是** |
| 前端源码 | `src/api/auth.ts` → `fetchGetUserInfo()` |
| 状态 | **已实现** |

**响应数据：**

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "userId": 1,
    "userName": "Admin",
    "email": "admin@example.com",
    "avatar": "https://example.com/avatar.jpg",
    "roles": ["R_ADMIN"],
    "buttons": ["btn_add", "btn_edit", "btn_delete"]
  }
}
```

> 详细说明见 `backend-api-guide.md` 5.2 节。

---

### 3. 用户注册 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/auth/register` |
| 需要 Token | 否 |
| 前端源码 | `src/views/auth/register/index.vue` 第 198 行（当前为 TODO） |

**请求参数：**

```typescript
interface RegisterParams {
  userName: string       // 用户名（3-20 个字符）
  password: string       // 密码（最少 6 个字符）
}
```

```json
{
  "userName": "newuser",
  "password": "123456"
}
```

> 注意：前端已在客户端做了 `confirmPassword` 验证（两次密码一致性），不会发送到后端。

**响应数据：**

```json
{
  "code": 200,
  "msg": "注册成功",
  "data": null
}
```

**后端注意事项：**
- 用户名唯一性校验，重复时返回 `409 Conflict` ProblemDetails
- 密码加密存储（推荐 bcrypt）
- 注册成功后前端会自动跳转登录页，无需返回 Token

---

### 4. 获取菜单列表

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/v3/system/menus` |
| 需要 Token | **是** |
| 前端源码 | `src/api/system-manage.ts` → `fetchGetMenuList()` |
| 状态 | **已实现** |

> 详细说明见 `backend-api-guide.md` 5.3 节。仅当 `VITE_ACCESS_MODE = backend` 时前端才调用。

---

## P1 — 系统管理 CRUD

> 系统管理模块的核心 CRUD 操作。前端页面 UI 已完成，但提交操作都是 TODO 占位符。

### 5. 用户列表（已定义）

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/user/list` |
| 需要 Token | **是** |
| 前端源码 | `src/api/system-manage.ts` → `fetchGetUserList()` |
| 状态 | **已实现** |

> 详细说明见 `backend-api-guide.md` 5.4 节。

---

### 6. 新增用户 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/user` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/user/modules/user-dialog.vue` 第 132 行（当前为 TODO） |

**请求参数：**

```typescript
interface CreateUserParams {
  username: string       // 用户名（必填，2-20 个字符）
  phone: string          // 手机号（必填，正则 /^1[3-9]\d{9}$/）
  gender: string         // 性别（必填，"男" 或 "女"）
  role: string[]         // 角色编码列表（必填，如 ["R_ADMIN", "R_USER"]）
}
```

```json
{
  "username": "newuser",
  "phone": "13800138000",
  "gender": "男",
  "role": ["R_ADMIN"]
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "添加成功",
  "data": null
}
```

**后端注意事项：**
- 用户名唯一性校验
- 创建用户时需要设置默认密码（建议可配置）
- 角色编码需要校验有效性

---

### 7. 编辑用户 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/user/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/user/modules/user-dialog.vue`（编辑模式） |

**请求参数：**

```json
{
  "username": "admin",
  "phone": "13800138000",
  "gender": "男",
  "role": ["R_ADMIN"]
}
```

> 字段同新增用户。`id` 通过 URL 路径传递。

**响应数据：**

```json
{
  "code": 200,
  "msg": "更新成功",
  "data": null
}
```

---

### 8. 删除用户 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `DELETE /api/user/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/user/index.vue` 第 231 行（当前只显示消息，无真实 API） |

**请求参数：** 无（`id` 通过 URL 路径传递）

**响应数据：**

```json
{
  "code": 200,
  "msg": "删除成功",
  "data": null
}
```

**后端注意事项：**
- 前端 UI 文案为"注销用户"，可根据业务决定是真实删除还是软删除（修改状态）
- 不允许删除自己

---

### 9. 角色列表（已定义）

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/role/list` |
| 需要 Token | **是** |
| 前端源码 | `src/api/system-manage.ts` → `fetchGetRoleList()` |
| 状态 | **已实现** |

> 详细说明见 `backend-api-guide.md` 5.5 节。

---

### 10. 新增角色 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/role` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/role/modules/role-edit-dialog.vue` 第 148 行（当前为 TODO） |

**请求参数：**

```typescript
interface CreateRoleParams {
  roleName: string       // 角色名称（必填，2-20 个字符）
  roleCode: string       // 角色编码（必填，2-50 个字符，如 "R_EDITOR"）
  description: string    // 角色描述（必填）
  enabled: boolean       // 是否启用（默认 true）
}
```

```json
{
  "roleName": "编辑者",
  "roleCode": "R_EDITOR",
  "description": "拥有系统管理权限",
  "enabled": true
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "新增成功",
  "data": null
}
```

---

### 11. 编辑角色 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/role/{roleId}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/role/modules/role-edit-dialog.vue`（编辑模式） |

**请求参数：** 同新增角色，`roleId` 通过 URL 路径传递。

**响应数据：**

```json
{
  "code": 200,
  "msg": "修改成功",
  "data": null
}
```

---

### 12. 删除角色 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `DELETE /api/role/{roleId}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/role/index.vue` 第 234 行（当前为 TODO） |

**请求参数：** 无

**响应数据：**

```json
{
  "code": 200,
  "msg": "删除成功",
  "data": null
}
```

**后端注意事项：**
- 如果角色已分配给用户，需要提示无法删除或做级联处理
- 内置角色（R_SUPER、R_ADMIN）建议禁止删除

---

### 13. 获取角色权限 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/role/{roleId}/permissions` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/role/modules/role-permission-dialog.vue` 第 146 行（当前为 TODO） |

**响应数据：**

```typescript
interface RolePermissions {
  /** 该角色已勾选的菜单 name 列表 */
  checkedKeys: string[]
  /** 半选状态的菜单 name 列表（父级部分选中） */
  halfCheckedKeys: string[]
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "checkedKeys": ["DashboardConsole", "DashboardAnalysis", "SystemUser"],
    "halfCheckedKeys": ["Dashboard", "System"]
  }
}
```

> 前端使用 Element Plus 的 `ElTree` 组件，`node-key` 为菜单的 `name` 字段。
> 后端返回该角色已勾选的菜单 name 列表即可，前端会自动处理树形展示。

---

### 14. 保存角色权限 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/role/{roleId}/permissions` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/role/modules/role-permission-dialog.vue` 第 163 行（当前为 TODO） |

**请求参数：**

```json
{
  "checkedKeys": ["DashboardConsole", "DashboardAnalysis", "SystemUser"],
  "halfCheckedKeys": ["Dashboard", "System"]
}
```

> `checkedKeys` 是完全选中的菜单 name 列表，`halfCheckedKeys` 是半选的父级菜单。
> 后端需要同时存储两种状态，以便回显时正确恢复树形选中。

**响应数据：**

```json
{
  "code": 200,
  "msg": "权限保存成功",
  "data": null
}
```

---

### 15. 新增菜单 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/system/menu` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/menu/index.vue` 第 416 行（当前为 TODO） |

**请求参数（菜单类型）：**

```typescript
interface CreateMenuParams {
  /** 菜单类型：menu=菜单, button=按钮权限 */
  menuType: 'menu' | 'button'

  // ---- 以下为 menuType='menu' 时的字段 ----
  name: string           // 菜单名称（必填）
  path: string           // 路由地址（必填）
  label: string          // 权限标识 / 路由 name
  component?: string     // 组件路径（如 "system/user/index"）
  icon?: string          // 图标（如 "ri:user-line"）
  sort?: number          // 排序（默认 1）
  parentId?: number      // 父级菜单 ID（0 为一级菜单）
  isEnable?: boolean     // 是否启用（默认 true）
  keepAlive?: boolean    // 页面缓存
  isHide?: boolean       // 隐藏菜单
  isHideTab?: boolean    // 隐藏标签页
  link?: string          // 外部链接
  isIframe?: boolean     // 是否内嵌
  showBadge?: boolean    // 显示徽章
  showTextBadge?: string // 文本徽章
  fixedTab?: boolean     // 固定标签
  activePath?: string    // 激活路径
  roles?: string[]       // 角色权限
  isFullPage?: boolean   // 全屏页面

  // ---- 以下为 menuType='button' 时的字段 ----
  authName?: string      // 权限名称（如 "新增"）
  authLabel?: string     // 权限标识（如 "add"）
  authSort?: number      // 权限排序
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "新增成功",
  "data": null
}
```

---

### 16. 编辑菜单 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/system/menu/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/menu/modules/menu-dialog.vue`（编辑模式） |

**请求参数：** 同新增菜单，`id` 通过 URL 路径传递。

---

### 17. 删除菜单 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `DELETE /api/system/menu/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/system/menu/index.vue` 第 425 行 |

**请求参数：** 无

**响应数据：**

```json
{
  "code": 200,
  "msg": "删除成功",
  "data": null
}
```

**后端注意事项：**
- 删除菜单时应级联删除子菜单和关联的权限按钮
- 已分配给角色的菜单删除后需要清理关联关系

---

## P2 — 系统参数配置

> 用于“系统参数配置”功能：集中管理 key/value 配置（开关、阈值、业务常量等）。
> 建议后端做两层：DB 持久化 + 缓存（可选），并提供刷新接口。

### 18. 参数列表（分页）(NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/system/config/list` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数（Query String）：**

```typescript
interface ConfigQueryParams {
  current?: number       // 页码（默认 1）
  size?: number          // 每页条数（默认 20）
  configName?: string    // 参数名称（模糊）
  configKey?: string     // 参数 key（模糊）
  enabled?: boolean      // 是否启用
}
```

**响应数据：**

```typescript
interface ConfigItem {
  id: number
  configName: string
  configKey: string
  configValue: string
  remark?: string
  enabled: boolean
  isSystem: boolean
  createTime: string
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "records": [],
    "current": 1,
    "size": 20,
    "total": 0
  }
}
```

---

### 19. 新增参数 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/system/config` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数：**

```json
{
  "configName": "登录失败锁定阈值",
  "configKey": "auth.login.fail.lockThreshold",
  "configValue": "10",
  "remark": "连续失败次数达到阈值触发锁定",
  "enabled": true
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "新增成功",
  "data": null
}
```

**后端注意事项：**
- `configKey` 全局唯一，重复时返回 `409 Conflict` ProblemDetails
- `isSystem=true` 的参数建议禁止删除（是否允许修改 key/name 需业务约束）

---

### 20. 编辑参数 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/system/config/{id}` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数：** 同新增参数，`id` 通过 URL 路径传递。

---

### 21. 删除参数 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `DELETE /api/system/config/{id}` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**后端注意事项：**
- `isSystem=true` 禁止删除
- 删除前可选：检查是否被关键业务依赖（做成 warning/二次确认）

---

### 22. 按 key 获取参数值（供业务读取）(NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/system/config/by-key/{configKey}` |
| 需要 Token | **是**（建议：仅管理员/内部服务可读） |
| 状态 | **规划中** |

**响应数据：**

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "configKey": "auth.login.fail.lockThreshold",
    "configValue": "10"
  }
}
```

---

### 23. 刷新参数缓存（可选）(NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/system/config/refresh` |
| 需要 Token | **是** |
| 状态 | **规划中** |

> 用于 DB 更新后让应用立即加载最新参数（如实现了内存/Redis 缓存）。

---

## P3 — 定时任务管理 / 运行记录

> 本项目当前定时任务以代码内 `#[cron]` 定义为主（如 S3 分片清理）。
> 因此建议分两阶段：
> - **阶段 A（推荐先做）**：管理“已注册任务”（列表/启停/手动触发/运行记录）
> - **阶段 B（可选）**：支持“可配置任务”（UI 新增/修改/删除），需要额外决策 handler 机制与安全边界

### 24. 任务列表（分页）(NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/job/list` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数（Query String）：**

```typescript
interface JobQueryParams {
  current?: number
  size?: number
  jobName?: string
  enabled?: boolean
}
```

**响应数据（示例字段）：**

```typescript
interface JobItem {
  id: number
  jobName: string
  cron: string
  enabled: boolean
  lastRunTime?: string
  nextRunTime?: string
}
```

---

### 25. 启用/停用任务 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/job/{id}/enabled` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数：**

```json
{
  "enabled": false
}
```

---

### 26. 手动触发一次 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/job/{id}/run` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**响应数据：**

```json
{
  "code": 200,
  "msg": "已触发执行",
  "data": null
}
```

---

### 27. 任务运行记录列表（分页）(NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/job/log/list` |
| 需要 Token | **是** |
| 前端源码 | —（新页面，待补充） |
| 状态 | **规划中** |

**请求参数（Query String）：**

```typescript
interface JobLogQueryParams {
  current?: number
  size?: number
  jobId?: number
  status?: 'success' | 'failed'
  startTime?: string
  endTime?: string
}
```

---

### 28. 任务运行记录详情 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/job/log/{id}` |
| 需要 Token | **是** |
| 状态 | **规划中** |

---

## 基础能力（已完成）

### 通用文件上传（已实现）

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/file/upload` |
| Content-Type | `multipart/form-data`（浏览器自动设置） |
| 需要 Token | **是** |
| 状态 | **已实现** |

> 如需前端直传，可使用 presigned 系列接口（`/api/file/presign/upload` 等）。

---

## 待定接口（前端 UI 已预留，需确认业务需求）

以下接口前端页面有对应的 UI 入口，但功能较简单或业务逻辑未确定：

| 接口 | 前端位置 | 说明 |
|------|---------|------|
| `POST /api/auth/forget-password` | `src/views/auth/forget-password/index.vue` | 忘记密码，当前页面仅有用户名输入框，无验证码或邮箱验证流程。**建议确认重置密码的验证方式后再实现。** |
| `POST /api/auth/refresh` | — | Token 刷新接口。前端已预留 `refreshToken` 字段但未实现自动刷新逻辑。见 `backend-api-guide.md` 7.3 节。 |

---

## 数据库表设计建议

基于前端数据结构，后端至少需要以下数据表：

| 表名 | 说明 | 关键字段 |
|------|------|---------|
| `sys_user` | 用户表 | id, userName, password, phone, gender, email, avatar, status, createTime |
| `sys_role` | 角色表 | id, roleName, roleCode, description, enabled, createTime |
| `sys_user_role` | 用户-角色关联表 | id, userId, roleId |
| `sys_menu` | 菜单表 | id, parentId, name, path, component, icon, sort, menuType(menu/button), enabled, ... |
| `sys_role_menu` | 角色-菜单关联表 | id, roleId, menuId |
| `sys_config` | 系统参数配置表 | id, configName, configKey, configValue, enabled, isSystem, remark, createTime, updateTime |
| `sys_job` | 定时任务表 | id, jobName, cron, handler, params, enabled, remark, createTime, updateTime |
| `sys_job_log` | 任务运行记录表 | id, jobId, startTime, endTime, status, durationMs, error, createTime |

---

## 开发顺序建议

```
第 1 步：实现 P0 认证接口
         POST /api/auth/login
         GET /api/user/info
         POST /api/auth/register
         GET /api/v3/system/menus（后端模式下）
         ↓
第 2 步：联调核心流程
         登录 → 获取用户信息 → 加载菜单 → 页面渲染
         ↓
第 3 步：实现 P1 系统管理接口
         用户 CRUD → 角色 CRUD → 角色权限分配 → 菜单 CRUD
         ↓
第 4 步：实现 P2 系统参数配置
         参数列表 → 新增/编辑/删除 → 按 key 获取/刷新缓存
         ↓
第 5 步：实现 P3 定时任务管理 / 运行记录
         任务列表/启停/手动触发 → 运行记录列表/详情
```

---

## 接口汇总表

| # | 方法 | 路径 | 说明 | 优先级 | 状态 |
|:-:|------|------|------|:------:|:----:|
| 1 | POST | `/api/auth/login` | 用户登录 | P0 | 已实现 |
| 2 | GET | `/api/user/info` | 获取用户信息 | P0 | 已实现 |
| 3 | POST | `/api/auth/register` | 用户注册 | P0 | 待实现 |
| 4 | GET | `/api/v3/system/menus` | 获取菜单列表 | P0 | 已实现 |
| 5 | GET | `/api/user/list` | 用户列表（分页） | P1 | 已实现 |
| 6 | POST | `/api/user` | 新增用户 | P1 | 已实现 |
| 7 | PUT | `/api/user/{id}` | 编辑用户 | P1 | 已实现 |
| 8 | DELETE | `/api/user/{id}` | 删除用户 | P1 | 已实现 |
| 9 | GET | `/api/role/list` | 角色列表（分页） | P1 | 已实现 |
| 10 | POST | `/api/role` | 新增角色 | P1 | 已实现 |
| 11 | PUT | `/api/role/{roleId}` | 编辑角色 | P1 | 已实现 |
| 12 | DELETE | `/api/role/{roleId}` | 删除角色 | P1 | 已实现 |
| 13 | GET | `/api/role/{roleId}/permissions` | 获取角色权限 | P1 | 已实现 |
| 14 | PUT | `/api/role/{roleId}/permissions` | 保存角色权限 | P1 | 已实现 |
| 15 | POST | `/api/system/menu` | 新增菜单 | P1 | 已实现 |
| 16 | PUT | `/api/system/menu/{id}` | 编辑菜单 | P1 | 已实现 |
| 17 | DELETE | `/api/system/menu/{id}` | 删除菜单 | P1 | 已实现 |
| 18 | GET | `/api/system/config/list` | 系统参数列表（分页） | P2 | 规划中 |
| 19 | POST | `/api/system/config` | 新增系统参数 | P2 | 规划中 |
| 20 | PUT | `/api/system/config/{id}` | 编辑系统参数 | P2 | 规划中 |
| 21 | DELETE | `/api/system/config/{id}` | 删除系统参数 | P2 | 规划中 |
| 22 | GET | `/api/system/config/by-key/{configKey}` | 按 key 获取参数值 | P2 | 规划中 |
| 23 | POST | `/api/system/config/refresh` | 刷新参数缓存（可选） | P2 | 规划中 |
| 24 | GET | `/api/job/list` | 定时任务列表（分页） | P3 | 规划中 |
| 25 | PUT | `/api/job/{id}/enabled` | 启用/停用定时任务 | P3 | 规划中 |
| 26 | POST | `/api/job/{id}/run` | 手动触发一次 | P3 | 规划中 |
| 27 | GET | `/api/job/log/list` | 任务运行记录列表 | P3 | 规划中 |
| 28 | GET | `/api/job/log/{id}` | 任务运行记录详情 | P3 | 规划中 |

---

*文档版本：v1.1*
*更新日期：2026-03-13*
*适用前端版本：art-design-pro v3.0.1*
