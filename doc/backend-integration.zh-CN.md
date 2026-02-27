# 后端联调与接口约定（Art Design Pro / Vue3）

本文档用于后端同学在接口尚未正式开发阶段，提前明确：**前端请求方式、鉴权方式、统一响应结构、分页结构、菜单/路由数据结构**等联调约定，避免后期反复返工。

> 以当前前端代码为准：`src/utils/http/`、`src/api/`、`src/types/api/api.d.ts`、`src/types/router/index.ts`。

---

## 1. 联调方式（本地开发）

前端开发环境使用 Vite 代理转发 `/api`：

- 前端请求 `baseURL`：`VITE_API_URL`（`.env.development` 默认是 `/`）
- Vite 代理：`vite.config.ts` 将 **`/api`** 转发到 `VITE_API_PROXY_URL`

你只需要确保后端服务地址与 `.env.development` 对齐：

- 修改 `.env.development`：
  - `VITE_API_PROXY_URL = http://localhost:8080`（示例）
- 后端接口统一以 `/api` 作为前缀，例如：`/api/auth/login`

### 1.1 跨域与 Cookie（可选）

前端是否携带 Cookie 由环境变量控制：

- `VITE_WITH_CREDENTIALS = true/false`（见 `.env`、`src/utils/http/index.ts`）

如后端选择 Cookie 会话方案或需要跨域携带 Cookie，请确保后端正确配置 CORS（至少包含）：

- `Access-Control-Allow-Origin`：指定明确的前端域名（**不能是 `*`**）
- `Access-Control-Allow-Credentials: true`
- `Access-Control-Allow-Headers`：包含 `Authorization, Content-Type`

---

## 2. 鉴权约定（Token）

### 2.1 Token 下发

登录接口成功后，前端会保存：

- `token`（访问令牌）
- `refreshToken`（刷新令牌，当前前端尚未实现刷新流程，但字段已预留）

见：`src/api/auth.ts`、`src/types/api/api.d.ts`。

### 2.2 Token 传递方式

前端在每次请求的请求头自动携带：

- Header：`Authorization: <accessToken>`

见：`src/utils/http/index.ts`。

建议后端兼容两种形式（任选其一即可）：

1) 直接 token：`Authorization: xxxxxx`  
2) Bearer：`Authorization: Bearer xxxxxx`（推荐；也可让后端在登录时直接返回带 `Bearer ` 前缀的 `token` 字符串，前端会原样保存并发送）

---

## 3. 统一响应结构（非常重要）

前端 Axios 封装要求后端响应为 JSON，且具备统一结构：

```json
{
  "code": 200,
  "msg": "ok",
  "data": {}
}
```

- `code`：业务状态码（**与 HTTP 状态码保持一致**，见 `src/utils/http/status.ts`）
- `msg`：提示信息（前端会直接用于错误提示）
- `data`：业务数据（前端请求函数最终只返回 `data`，见 `src/utils/http/index.ts`）

### 3.1 code 状态码约定

前端内置状态码枚举（见 `src/utils/http/status.ts`），建议后端至少对齐以下值：

- `200`：成功
- `400`：业务错误（参数错误/校验失败等，建议用 `msg` 提供可读原因）
- `401`：未登录 / Token 失效（前端会自动登出并跳转登录）
- `403`：无权限
- `404`：资源不存在
- `500`：服务器内部错误

### 3.2 HTTP 状态码建议

由于前端 Axios 配置了 `validateStatus: 2xx`（见 `src/utils/http/index.ts`），为减少“网络错误/统一错误文案”带来的信息丢失，建议：

- **业务失败也返回 HTTP 200**，通过响应体 `code != 200` 表达错误
- 未登录/Token 失效建议返回：
  - HTTP 200 + `code = 401`
  - `msg` 给出可读原因（例如：`"登录已过期"`）

> 当前前端会在 `code === 401` 时自动登出并跳转登录页。

---

## 4. 请求格式约定

- `Content-Type: application/json`
- `POST/PUT`：前端会将 `params` 自动作为 JSON Body 发送（见 `src/utils/http/index.ts`）
- `GET`：参数走 QueryString（`?a=1&b=2`）
- 文件上传：如使用 `FormData`，前端会跳过 JSON stringify（后端按 multipart/form-data 处理即可）

---

## 5. 分页结构约定（列表接口）

### 5.1 请求参数

前端默认分页参数名：

- `current`：当前页（从 1 开始）
- `size`：每页条数

见：`src/utils/table/tableConfig.ts`、`src/types/api/api.d.ts`。

### 5.2 响应数据（data 字段）

建议统一为：

```json
{
  "records": [],
  "current": 1,
  "size": 20,
  "total": 100
}
```

对应类型：`Api.Common.PaginatedResponse<T>`（`src/types/api/api.d.ts`）。

---

## 6. 当前前端已使用的接口清单（MVP）

> 路径以当前前端代码 `src/api/*.ts` 为准。

### 6.1 登录

- `POST /api/auth/login`
- Body（JSON）：

```json
{
  "userName": "admin",
  "password": "123456"
}
```

- Success `data`：

```json
{
  "token": "Bearer xxx",
  "refreshToken": "xxx"
}
```

类型：`Api.Auth.LoginParams` / `Api.Auth.LoginResponse`（`src/types/api/api.d.ts`）。

### 6.2 获取当前用户信息

- `GET /api/user/info`
- Header：`Authorization`
- Success `data`（示例）：

```json
{
  "buttons": ["add", "edit", "delete"],
  "roles": ["admin"],
  "userId": 1,
  "userName": "admin",
  "email": "admin@example.com",
  "avatar": "https://example.com/avatar.png"
}
```

类型：`Api.Auth.UserInfo`（`src/types/api/api.d.ts`）。

### 6.3 用户列表

- `GET /api/user/list`
- Query（示例）：
  - `current=1&size=20&userName=...&status=1`
- Success `data`：`Api.SystemManage.UserList`（分页结构）

字段参考：`Api.SystemManage.UserListItem`（`src/types/api/api.d.ts`）。

### 6.4 角色列表

- `GET /api/role/list`
- Query（示例）：
  - `current=1&size=20&roleName=...`
- Success `data`：`Api.SystemManage.RoleList`（分页结构）

字段参考：`Api.SystemManage.RoleListItem`（`src/types/api/api.d.ts`）。

### 6.5 菜单/路由（后端权限模式）

- `GET /api/v3/system/menus`
- Success `data`：`AppRouteRecord[]`（见 `src/types/router/index.ts`）

该接口仅在 **`VITE_ACCESS_MODE=backend`** 时会被调用（见 `src/hooks/core/useAppMode.ts`、`src/router/core/MenuProcessor.ts`）。

---

## 7. 菜单/路由数据结构约定（后端返回）

### 7.1 结构（关键字段）

`AppRouteRecord`（简化）：

```ts
interface AppRouteRecord {
  path: string
  name?: string
  component?: string
  meta: {
    title: string
    icon?: string
    roles?: string[]
    isHide?: boolean
    keepAlive?: boolean
    link?: string
    isIframe?: boolean
    authList?: Array<{ title: string; authMark: string }>
    // ...更多字段见 src/types/router/index.ts
  }
  children?: AppRouteRecord[]
}
```

### 7.2 path 规则（非常重要）

前端会校验并自动规范化路径（见 `src/router/core/MenuProcessor.ts`）：

- 一级菜单：`path` 建议以 `/` 开头，例如：`/system`
- 二级及以下菜单：
  - **不要以 `/` 开头**，使用相对路径，例如：`user`、`role`
  - 例：父级 `/system` + 子级 `user` => 最终路径 `/system/user`
- 允许例外（可以以 `/` 开头）：
  - 外链：`http://` / `https://`
  - iframe 路由：`/outside/iframe/` 开头

### 7.3 component 规则（非常重要）

前端会做组件配置校验与动态加载（见 `src/router/core/RouteValidator.ts`、`src/router/core/ComponentLoader.ts`）：

- 一级菜单（目录/模块入口）：
  - 必须配置 `component: "/index/index"`（即 `RoutesAlias.Layout`）
  - 除非该菜单是外链（`meta.link`）或 iframe（`meta.isIframe`）
- 二级及以下菜单：
  - `component` 应指向具体页面组件路径，例如：`/system/user`
  - 前端会尝试加载：
    - `src/views/system/user.vue`
    - 或 `src/views/system/user/index.vue`
  - 二级及以下菜单 **不能** 使用 `"/index/index"` 作为 component
- 目录菜单（仅用于分组、不对应页面）：
  - 可将 `component` 设为 `""` 或不传，并提供 `children`

### 7.4 按钮权限（authList）

在 **后端权限模式** 下，按钮/操作权限来自路由的 `meta.authList`：

- `meta.authList: [{ title: '新增', authMark: 'add' }]`
- 前端权限判断：
  - `useAuth()`：检查当前路由 `meta.authList` 是否包含 `authMark`
  - `v-auth` 指令：同上

见：`src/hooks/core/useAuth.ts`、`src/directives/core/auth.ts`。

---

## 8. 常见字段与格式建议

- 时间字段：使用字符串（建议 ISO8601 或 `YYYY-MM-DD HH:mm:ss`），例如：`"2026-02-26 16:00:00"`
- ID：建议数字（与前端类型保持一致）
- 空列表：返回 `[]`，不要返回 `null`
- `msg`：尽量可读、可直接展示给用户（前端会作为提示内容）

---

## 9. 类型参考（前端权威定义）

后端在设计返回字段时，可直接对照前端类型文件：

- 统一响应结构：`src/types/common/response.ts`
- API 参数/返回类型：`src/types/api/api.d.ts`
- 菜单/路由类型：`src/types/router/index.ts`
- 前端请求封装与状态码：`src/utils/http/index.ts`、`src/utils/http/status.ts`
