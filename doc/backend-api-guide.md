# Art Design Pro 后端接口联调指南

> 本文档面向后端开发者，详细描述前端项目对后端接口的格式要求、认证机制、数据结构等规范。
> 后端开发接口前请务必通读本文档，确保前后端数据格式一致。

---

## 一、项目基本信息

| 项目 | 说明 |
|------|------|
| 前端框架 | Vue 3.5 + TypeScript 5.6 |
| UI 组件库 | Element Plus 2.11 |
| HTTP 客户端 | Axios（封装在 `src/utils/http/`） |
| 状态管理 | Pinia 3（持久化到 localStorage） |
| 权限模式 | `frontend`（前端控制）/ `backend`（后端控制），通过 `.env` 中 `VITE_ACCESS_MODE` 切换 |
| 开发端口 | 3006 |
| API 代理 | 开发环境通过 Vite 代理，所有 `/api` 开头的请求自动转发到后端 |

---

## 二、统一响应格式（最重要）

**所有接口必须遵循以下 JSON 响应结构：**

```typescript
interface BaseResponse<T = unknown> {
  code: number    // 业务状态码（不是 HTTP 状态码）
  msg: string     // 提示消息（成功/错误信息，前端会直接展示给用户）
  data: T         // 实际业务数据
}
```

### 成功响应示例

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": {
    "userId": 1,
    "userName": "admin"
  }
}
```

### 错误响应示例

```json
{
  "code": 400,
  "msg": "用户名或密码错误",
  "data": null
}
```

### 前端处理逻辑

```
收到响应
  ├─ response.data.code === 200  →  请求成功，返回 data 字段
  ├─ response.data.code === 401  →  Token 失效，自动登出，跳转登录页
  └─ response.data.code === 其他  →  请求失败，展示 msg 字段内容作为错误提示
```

> **关键点：** 前端判断请求成功的唯一标准是 `code === 200`，不是 HTTP Status Code。即使 HTTP 返回 200，如果 body 中 `code !== 200`，前端也会当作失败处理。

---

## 三、业务状态码定义

前端已定义的状态码（后端应保持一致）：

| 状态码 | 含义 | 前端处理方式 |
|--------|------|-------------|
| `200` | 成功 | 返回 data 数据 |
| `400` | 请求错误/参数错误 | 显示 msg 提示 |
| `401` | 未授权/Token 过期 | **自动登出，跳转登录页**（3秒防抖） |
| `403` | 禁止访问/权限不足 | 显示错误提示 |
| `404` | 资源不存在 | 显示错误提示 |
| `405` | 请求方法不允许 | 显示错误提示 |
| `408` | 请求超时 | 显示错误提示（可重试） |
| `500` | 服务器内部错误 | 显示错误提示（可重试） |
| `502` | 网关错误 | 显示错误提示（可重试） |
| `503` | 服务不可用 | 显示错误提示（可重试） |
| `504` | 网关超时 | 显示错误提示（可重试） |

> **401 特殊处理：** 前端收到 `code: 401` 后会执行防抖登出（3秒内多个 401 只处理一次），清空本地 Token，并跳转到登录页。

---

## 四、请求头规范

### 4.1 前端自动添加的请求头

```http
Authorization: {token值}
Content-Type: application/json
```

- `Authorization`：前端从本地存储中读取 `accessToken`，直接放入请求头（**不会自动加 `Bearer ` 前缀**）
- `Content-Type`：非 FormData 请求自动设为 `application/json`
- FormData 请求（如文件上传）：前端**不设置** Content-Type，由浏览器自动处理 multipart boundary

### 4.2 后端需要注意

- 从 `Authorization` 请求头读取 Token 时，注意前端发送的是**裸 Token 值**
- 如果后端需要 `Bearer` 前缀格式（如 `Bearer eyJxxx...`），有两种方案：
  - **方案 A**（推荐）：后端登录接口返回 token 时就带上 `Bearer ` 前缀
  - **方案 B**：后端解析时自行处理有无 `Bearer ` 前缀的情况

> 源码参考：`src/utils/http/index.ts` 第 68 行
> ```typescript
> if (accessToken) request.headers.set('Authorization', accessToken)
> ```

---

## 五、接口清单

### 5.1 登录接口

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/auth/login` |
| Content-Type | `application/json` |
| 是否需要 Token | 否 |
| 超时时间 | 15 秒 |

**请求参数：**

```typescript
interface LoginParams {
  userName: string   // 用户名
  password: string   // 密码
}
```

```json
{
  "userName": "admin",
  "password": "123456"
}
```

**响应数据：**

```typescript
interface LoginResponse {
  token: string         // 访问令牌（必须）
  refreshToken: string  // 刷新令牌（必须）
}
```

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

**前端登录流程：**

```
1. 用户输入用户名+密码 → 调用 POST /api/auth/login
2. 拿到 token + refreshToken → 存入 Pinia Store（自动持久化到 localStorage）
3. 设置登录状态 isLogin = true
4. 跳转到首页（或 redirect 参数指定的页面）
5. 路由守卫触发 → 调用 GET /api/user/info 获取用户信息
6. 调用 GET /api/v3/system/menus 获取菜单列表（后端模式下）
7. 动态注册路由，渲染菜单
```

---

### 5.2 获取用户信息接口

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/user/info` |
| 是否需要 Token | **是** |
| 调用时机 | 登录后首次路由跳转时自动调用 |

**请求参数：** 无（通过 Token 识别用户）

**响应数据：**

```typescript
interface UserInfo {
  userId: number       // 用户ID（必须，用于多账号工作台切换判断）
  userName: string     // 用户名（必须）
  email: string        // 邮箱（必须）
  avatar?: string      // 头像URL（可选）
  roles: string[]      // 角色列表（必须，用于权限控制）
  buttons: string[]    // 按钮权限标识列表（必须，用于按钮级权限控制）
}
```

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
    "buttons": ["btn_add", "btn_edit", "btn_delete", "btn_export"]
  }
}
```

**roles 字段说明：**

前端使用 `roles` 做菜单和路由的权限过滤。前端目前预设的角色标识为：

| 角色标识 | 含义 |
|---------|------|
| `R_SUPER` | 超级管理员 |
| `R_ADMIN` | 管理员 |
| `R_USER` | 普通用户 |

> 后端可以自定义角色标识，只需要保证菜单路由的 `meta.roles` 和用户的 `roles` 字段匹配即可。

**buttons 字段说明：**

用于按钮级别的权限控制。前端通过 `v-auth` 指令或 `useAuth` hook 判断用户是否有某个操作权限。值为字符串数组，后端自定义即可。

---

### 5.3 获取菜单列表接口

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/v3/system/menus` |
| 是否需要 Token | **是** |
| 调用时机 | 登录后首次路由跳转时自动调用（仅 `VITE_ACCESS_MODE = backend` 时） |

> **注意：** 当 `VITE_ACCESS_MODE = frontend` 时，前端使用本地定义的路由（`src/router/routes/asyncRoutes.ts`），不会调用此接口。需要后端控制菜单时，将 `.env` 中 `VITE_ACCESS_MODE` 改为 `backend`。

**请求参数：** 无

**响应数据：** 树形菜单结构

```typescript
interface AppRouteRecord {
  path: string               // 路由路径
  name?: string              // 路由名称（唯一标识，推荐必填）
  component?: string         // 组件路径（相对于 src/views 目录，如 "dashboard/console/index"）
  redirect?: string          // 重定向路径
  children?: AppRouteRecord[] // 子菜单
  meta: {
    title: string            // 菜单标题（必须）
    icon?: string            // 菜单图标
    isHide?: boolean         // 是否在菜单中隐藏（默认 false）
    isHideTab?: boolean      // 是否在标签页中隐藏
    link?: string            // 外部链接URL
    isIframe?: boolean       // 是否为iframe嵌入
    keepAlive?: boolean      // 是否缓存页面
    roles?: string[]         // 允许访问的角色列表（为空或不设置则不限制）
    isFirstLevel?: boolean   // 是否为一级菜单（无子菜单的独立页面）
    fixedTab?: boolean       // 是否固定标签页
    activePath?: string      // 高亮的菜单路径
    isFullPage?: boolean     // 是否全屏页面
    showBadge?: boolean      // 是否显示徽章
    showTextBadge?: string   // 文本徽章内容
    authList?: Array<{       // 操作权限列表
      title: string
      authMark: string
    }>
  }
}
```

**响应示例（树形结构）：**

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": [
    {
      "path": "/dashboard",
      "name": "Dashboard",
      "redirect": "/dashboard/console",
      "meta": {
        "title": "仪表盘",
        "icon": "dashboard"
      },
      "children": [
        {
          "path": "console",
          "name": "DashboardConsole",
          "component": "dashboard/console/index",
          "meta": {
            "title": "控制台"
          }
        },
        {
          "path": "analysis",
          "name": "DashboardAnalysis",
          "component": "dashboard/analysis/index",
          "meta": {
            "title": "分析页"
          }
        }
      ]
    },
    {
      "path": "/system",
      "name": "System",
      "redirect": "/system/user",
      "meta": {
        "title": "系统管理",
        "icon": "system",
        "roles": ["R_SUPER", "R_ADMIN"]
      },
      "children": [
        {
          "path": "user",
          "name": "SystemUser",
          "component": "system/user/index",
          "meta": {
            "title": "用户管理",
            "roles": ["R_SUPER", "R_ADMIN"]
          }
        },
        {
          "path": "role",
          "name": "SystemRole",
          "component": "system/role/index",
          "meta": {
            "title": "角色管理",
            "roles": ["R_SUPER"]
          }
        }
      ]
    }
  ]
}
```

**路径规则：**

| 层级 | path 格式 | 示例 |
|------|----------|------|
| 一级菜单 | 以 `/` 开头的绝对路径 | `/dashboard`、`/system` |
| 二级及以下菜单 | **不以 `/` 开头**的相对路径 | `console`、`user` |
| 外部链接 | 完整 URL | `https://github.com` |

> **重要：** 子菜单的 `path` 不要以 `/` 开头！前端会自动拼接父路径。例如父级 `/system` + 子级 `user` = `/system/user`。

**component 字段说明：**

- 值为 `src/views/` 目录下的相对路径
- 例如 `"dashboard/console/index"` 对应 `src/views/dashboard/console/index.vue`
- 一级菜单（有子菜单的）通常不需要 `component` 字段，前端会自动使用 Layout 组件
- 如果是外链（`meta.link` 有值），也不需要 `component`

---

### 5.4 获取用户列表接口

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/user/list` |
| 是否需要 Token | **是** |

**请求参数（Query String）：**

```typescript
interface UserSearchParams {
  // 分页参数
  current?: number       // 当前页码（从 1 开始，默认 1）
  size?: number          // 每页条数（默认 10）

  // 搜索条件（全部可选）
  id?: number            // 用户ID
  userName?: string      // 用户名
  userGender?: string    // 性别
  userPhone?: string     // 手机号
  userEmail?: string     // 邮箱
  status?: string        // 状态
}
```

```
GET /api/user/list?current=1&size=10&userName=admin
```

**响应数据：**

```typescript
interface PaginatedResponse<T> {
  records: T[]      // 数据列表
  current: number   // 当前页码
  size: number      // 每页条数
  total: number     // 总记录数
}

interface UserListItem {
  id: number
  avatar: string         // 头像URL
  status: string         // 状态（"1"=启用, "2"=禁用）
  userName: string       // 用户名
  userGender: string     // 性别
  nickName: string       // 昵称
  userPhone: string      // 手机号
  userEmail: string      // 邮箱
  userRoles: string[]    // 角色列表
  createBy: string       // 创建人
  createTime: string     // 创建时间
  updateBy: string       // 更新人
  updateTime: string     // 更新时间
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "records": [
      {
        "id": 1,
        "avatar": "https://example.com/avatar.jpg",
        "status": "1",
        "userName": "admin",
        "userGender": "male",
        "nickName": "管理员",
        "userPhone": "13800138000",
        "userEmail": "admin@example.com",
        "userRoles": ["R_ADMIN"],
        "createBy": "system",
        "createTime": "2024-01-01 12:00:00",
        "updateBy": "admin",
        "updateTime": "2024-06-15 10:30:00"
      }
    ],
    "current": 1,
    "size": 10,
    "total": 56
  }
}
```

---

### 5.5 获取角色列表接口

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/role/list` |
| 是否需要 Token | **是** |

**请求参数（Query String）：**

```typescript
interface RoleSearchParams {
  current?: number       // 页码（从 1 开始）
  size?: number          // 每页条数
  roleId?: number        // 角色ID
  roleName?: string      // 角色名称
  roleCode?: string      // 角色编码
  description?: string   // 描述
  enabled?: boolean      // 是否启用
}
```

**响应数据：**

```typescript
interface RoleListItem {
  roleId: number
  roleName: string       // 角色名称
  roleCode: string       // 角色编码
  description: string    // 描述
  enabled: boolean       // 是否启用
  createTime: string     // 创建时间
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "records": [
      {
        "roleId": 1,
        "roleName": "超级管理员",
        "roleCode": "R_SUPER",
        "description": "拥有系统所有权限",
        "enabled": true,
        "createTime": "2024-01-01 00:00:00"
      },
      {
        "roleId": 2,
        "roleName": "管理员",
        "roleCode": "R_ADMIN",
        "description": "拥有大部分管理权限",
        "enabled": true,
        "createTime": "2024-01-01 00:00:00"
      }
    ],
    "current": 1,
    "size": 10,
    "total": 3
  }
}
```

---

## 六、分页规范

**所有分页接口统一遵循以下规范：**

### 6.1 请求参数

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `current` | number | 1 | 当前页码，**从 1 开始**（不是 0） |
| `size` | number | 10 | 每页条数 |

> 前端 `useTable` hook 支持自定义分页字段名（`paginationKey` 配置），但默认使用 `current` 和 `size`。建议后端保持这两个字段名。

### 6.2 响应格式

```typescript
interface PaginatedResponse<T> {
  records: T[]      // 当前页数据列表（必须）
  current: number   // 当前页码（必须）
  size: number      // 每页条数（必须）
  total: number     // 总记录数（必须，用于前端分页组件计算总页数）
}
```

### 6.3 边界情况

- `records` 为空数组 `[]` 时，前端显示"暂无数据"
- `total` 必须是准确的总记录数，不能返回 0 或不返回
- 请求页码超出范围时，建议返回最后一页数据或空数组

---

## 七、Token 认证机制

### 7.1 Token 存储和使用

```
登录成功
  ↓
后端返回 token + refreshToken
  ↓
前端存储到 Pinia Store → 自动持久化到 localStorage
  ↓
后续所有请求自动在 Header 中携带：Authorization: {token}
```

### 7.2 Token 过期处理

当前前端的处理方式：

1. 任何接口返回 `code: 401` → 前端自动执行登出
2. 清空本地 Token 和用户信息
3. 跳转到登录页（携带当前路由作为 redirect 参数）
4. 防抖机制：3 秒内只处理一次 401（避免多个并发请求同时触发登出）

### 7.3 Token 刷新（待实现）

前端已预留 `refreshToken` 字段，但当前**未实现自动刷新**。建议后端提供以下方案之一：

**方案 A — 短 Token + 刷新接口（推荐）：**

```
POST /api/auth/refresh
请求体：{ "refreshToken": "xxx" }
响应：{ "code": 200, "data": { "token": "新token", "refreshToken": "新refreshToken" } }
```

- accessToken 有效期：建议 2 小时
- refreshToken 有效期：建议 7 天

**方案 B — 长 Token：**

- 单一 token，有效期设置较长（如 7 天）
- 过期后直接返回 401，用户重新登录

---

## 八、跨域（CORS）配置

### 8.1 开发环境

- 前端通过 Vite Dev Server 代理解决跨域
- 所有 `/api` 开头的请求代理到 `VITE_API_PROXY_URL` 指定的地址
- **后端无需额外配置 CORS**（代理转发是同源的）

### 8.2 生产环境

前端直接请求后端地址（`.env.production` 中的 `VITE_API_URL`），后端需要配置 CORS：

```
Access-Control-Allow-Origin: https://your-frontend-domain.com
Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization
Access-Control-Max-Age: 3600
```

如果 `VITE_WITH_CREDENTIALS = true`（需要携带 Cookie），还需要：

```
Access-Control-Allow-Credentials: true
```

> 当前配置 `VITE_WITH_CREDENTIALS = false`，即不携带 Cookie。

---

## 九、请求超时和重试

| 配置项 | 值 | 说明 |
|--------|-----|------|
| 请求超时 | 15 秒 | 超时后前端显示"请求超时"提示 |
| 最大重试次数 | 0 | 当前默认不重试（可配置） |
| 可重试状态码 | 408, 500, 502, 503, 504 | 仅这些状态码会触发重试 |
| 重试间隔 | 1000ms | 每次重试等待 1 秒 |

---

## 十、文件上传

前端 HTTP 拦截器会自动检测 `FormData` 类型，此时：

- **不会**设置 `Content-Type`（由浏览器自动添加 `multipart/form-data; boundary=...`）
- **不会**对请求体做 `JSON.stringify`
- 仍然会携带 `Authorization` Token

后端需要支持 `multipart/form-data` 格式接收文件。

---

## 十一、环境变量速查

### .env（通用）

```env
VITE_VERSION = 3.0.1                        # 版本号
VITE_PORT = 3006                             # 开发服务端口
VITE_BASE_URL = /                            # 部署基础路径
VITE_ACCESS_MODE = frontend                  # 权限模式 frontend/backend
VITE_WITH_CREDENTIALS = false                # 是否携带 Cookie
```

### .env.development（开发环境）

```env
VITE_API_URL = /                             # API 基础路径（使用代理）
VITE_API_PROXY_URL = https://mock-server...  # 代理目标地址（改为后端实际地址）
```

### .env.production（生产环境）

```env
VITE_API_URL = https://api.your-domain.com   # 后端 API 完整地址
```

**后端开发时，修改 `.env.development` 中的 `VITE_API_PROXY_URL` 为本地后端地址即可：**

```env
VITE_API_PROXY_URL = http://localhost:8080
```

---

## 十二、接口开发检查清单

### 必须满足

- [ ] 所有接口响应包含 `code`、`msg`、`data` 三个字段
- [ ] 成功返回 `code: 200`
- [ ] Token 过期/无效返回 `code: 401`
- [ ] 登录接口返回 `token` 和 `refreshToken` 字段
- [ ] 用户信息接口返回 `userId`、`userName`、`email`、`roles`、`buttons` 字段
- [ ] 分页接口响应包含 `records`、`current`、`size`、`total` 字段
- [ ] 分页页码从 `1` 开始
- [ ] 菜单接口返回树形结构（后端模式下）
- [ ] 子菜单 `path` 不以 `/` 开头

### 建议满足

- [ ] 生产环境配置 CORS
- [ ] 密码传输使用加密（前端使用 crypto-js）
- [ ] 返回时间格式统一为 `YYYY-MM-DD HH:mm:ss`
- [ ] 大列表接口支持分页，避免一次性返回全量数据
- [ ] 错误 `msg` 使用中文，前端会直接展示给用户
- [ ] 接口支持 Gzip 压缩

---

## 十三、接口路径汇总

| 方法 | 路径 | 说明 | 需要 Token |
|------|------|------|:----------:|
| POST | `/api/auth/login` | 用户登录 | 否 |
| GET | `/api/user/info` | 获取当前用户信息 | 是 |
| GET | `/api/v3/system/menus` | 获取菜单列表（后端模式） | 是 |
| GET | `/api/user/list` | 获取用户列表（分页） | 是 |
| GET | `/api/role/list` | 获取角色列表（分页） | 是 |

> 以上是前端目前已定义的接口。后续新增接口请遵循同样的规范。

---

## 十四、联调流程建议

```
第一步：后端实现登录接口 POST /api/auth/login
        ↓
第二步：后端实现用户信息接口 GET /api/user/info
        ↓
第三步：前端修改 .env.development 中的代理地址指向后端
        ↓
第四步：联调登录 + 用户信息（确保能正常登录并获取用户数据）
        ↓
第五步：后端实现菜单接口 GET /api/v3/system/menus（如果使用后端模式）
        ↓
第六步：联调菜单加载 + 权限控制
        ↓
第七步：按业务需求逐步实现其他 CRUD 接口
```

---

## 十五、测试账号要求

联调阶段，后端需要提供以下测试账号：

| 角色 | 用户名 | 密码 | roles 字段值 |
|------|--------|------|-------------|
| 超级管理员 | Super | 123456 | `["R_SUPER"]` |
| 管理员 | Admin | 123456 | `["R_ADMIN"]` |
| 普通用户 | User | 123456 | `["R_USER"]` |

> 前端登录页默认预设了以上三个账号供快速切换测试。

---

## 十六、附录：前端关键源码索引

| 模块 | 文件路径 | 说明 |
|------|---------|------|
| HTTP 封装 | `src/utils/http/index.ts` | Axios 实例、拦截器、请求方法 |
| 错误处理 | `src/utils/http/error.ts` | HttpError 类、错误消息映射 |
| 状态码定义 | `src/utils/http/status.ts` | ApiStatus 枚举 |
| API 接口定义 | `src/api/auth.ts` | 登录、用户信息接口 |
| API 接口定义 | `src/api/system-manage.ts` | 用户列表、角色列表、菜单接口 |
| API 类型声明 | `src/types/api/api.d.ts` | 所有 API 类型定义（全局命名空间） |
| 响应类型 | `src/types/common/response.ts` | BaseResponse 定义 |
| 路由类型 | `src/types/router/index.ts` | AppRouteRecord、RouteMeta 定义 |
| 用户 Store | `src/store/modules/user.ts` | Token 管理、登录/登出逻辑 |
| 路由守卫 | `src/router/guards/beforeEach.ts` | 登录验证、动态路由注册流程 |
| 菜单处理 | `src/router/core/MenuProcessor.ts` | 前端/后端模式菜单获取 |
| 登录页面 | `src/views/auth/login/index.vue` | 登录表单、登录流程 |
| 环境变量 | `.env` / `.env.development` / `.env.production` | API 地址等配置 |

---

*文档版本：v1.0*
*更新日期：2026-02-26*
*适用前端版本：art-design-pro v3.0.1*