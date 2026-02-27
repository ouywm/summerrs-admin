# 后端接口开发需求清单

> 本文档梳理了前端所有页面所需的后端接口，按**优先级**排列。
> 后端开发请优先实现 P0 级接口，确保核心流程跑通后，再按优先级逐步实现其他接口。
>
> **前置文档：** 请先阅读 `docs/backend-api-guide.md`（响应格式、Token 机制、分页规范等基础约定）
> 和 `docs/frontend-adaptation-guide.md`（错误响应 ProblemDetails 格式说明）。

---

## 接口总览

| 优先级 | 模块 | 接口数量 | 说明 |
|:------:|------|:--------:|------|
| **P0** | 认证 | 3 | 登录、注册、获取用户信息 |
| **P0** | 菜单 | 1 | 获取菜单列表（后端权限模式必须） |
| **P1** | 用户管理 | 3 | 用户列表、新增、编辑、删除 |
| **P1** | 角色管理 | 5 | 角色 CRUD + 权限分配 |
| **P1** | 菜单管理 | 3 | 菜单 CRUD（后端权限模式） |
| **P2** | 文章 | 5 | 文章 CRUD + 分类列表 |
| **P2** | 文件上传 | 1 | 通用文件上传 |
| **P3** | 留言 | 2 | 留言列表 + 评论 |

---

## P0 — 核心认证（最高优先级）

> 没有这些接口，前端无法登录和使用。已在 `backend-api-guide.md` 中详细说明。

### 1. 用户登录

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/auth/login` |
| 需要 Token | 否 |
| 前端源码 | `src/api/auth.ts` → `fetchLogin()` |
| 状态 | **已定义，待后端实现** |

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
| 状态 | **已定义，待后端实现** |

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
| 状态 | **已定义，待后端实现** |

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
| 状态 | **已定义，待后端实现** |

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
| 状态 | **已定义，待后端实现** |

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
  "description": "拥有文章编辑权限",
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

## P2 — 文章模块

> 文章模块当前使用外部 JSON 文件和 Mock 数据。需要后端实现完整的文章 CRUD。

### 18. 获取文章列表 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/article/list` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/list/index.vue` 第 135 行（当前为 TODO） |

**请求参数（Query String）：**

```typescript
interface ArticleListParams {
  current?: number       // 页码（默认 1）
  size?: number          // 每页条数（默认 40）
  searchVal?: string     // 标题搜索关键词
  year?: string          // 年份筛选（如 "2024"，空字符串表示全部）
}
```

```
GET /api/article/list?current=1&size=40&searchVal=Vue&year=2024
```

**响应数据：**

```typescript
interface ArticleListItem {
  id: number              // 文章 ID
  title: string           // 文章标题
  home_img: string        // 封面图 URL
  type_name: string       // 分类名称
  blog_class: string      // 分类 ID
  create_time: string     // 创建时间（YYYY-MM-DD HH:mm:ss）
  count: number           // 阅读量
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
        "title": "Vue 3 组合式 API 实践",
        "home_img": "https://example.com/cover.jpg",
        "type_name": "前端",
        "blog_class": "1",
        "create_time": "2024-06-15 10:30:00",
        "count": 128
      }
    ],
    "current": 1,
    "size": 40,
    "total": 25
  }
}
```

---

### 19. 获取文章详情 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/article/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/detail/index.vue` + `src/views/article/publish/index.vue`（编辑模式） |

**响应数据：**

```typescript
interface ArticleDetail {
  id: number
  title: string           // 文章标题
  blog_class: string      // 分类 ID
  html_content: string    // 文章 HTML 内容（富文本）
  cover?: string          // 封面图 URL
  visible?: boolean       // 是否可见
  create_time: string     // 创建时间
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": {
    "id": 1,
    "title": "Vue 3 组合式 API 实践",
    "blog_class": "1",
    "html_content": "<h2>前言</h2><p>...</p>",
    "cover": "https://example.com/cover.jpg",
    "visible": true,
    "create_time": "2024-06-15 10:30:00"
  }
}
```

---

### 20. 发布文章 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/article` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/publish/index.vue` 第 234 行（当前为 TODO） |

**请求参数：**

```json
{
  "title": "Vue 3 组合式 API 实践",
  "type": 1,
  "content": "<h2>前言</h2><p>...</p>",
  "cover": "https://example.com/cover.jpg",
  "visible": true
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|:----:|------|
| `title` | string | 是 | 文章标题（最多 100 字符） |
| `type` | number | 是 | 文章分类 ID |
| `content` | string | 是 | 文章 HTML 内容（富文本编辑器输出） |
| `cover` | string | 是 | 封面图 URL（通过上传接口获得） |
| `visible` | boolean | 否 | 是否可见（默认 true） |

**响应数据：**

```json
{
  "code": 200,
  "msg": "文章发布成功",
  "data": null
}
```

---

### 21. 编辑文章 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `PUT /api/article/{id}` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/publish/index.vue` 第 264 行（当前为 TODO） |

**请求参数：** 同发布文章，`id` 通过 URL 路径传递。

---

### 22. 获取文章分类列表 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/article/types` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/publish/index.vue` 第 156 行（当前使用外部 JSON） |

**响应数据：**

```typescript
interface ArticleType {
  id: number       // 分类 ID
  name: string     // 分类名称
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": [
    { "id": 1, "name": "前端" },
    { "id": 2, "name": "后端" },
    { "id": 3, "name": "设计" }
  ]
}
```

> 注意：此接口返回的不是分页数据，而是全量列表（用于下拉选择）。

---

### 23. 通用文件上传 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/common/upload` |
| Content-Type | `multipart/form-data`（浏览器自动设置） |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/publish/index.vue` 第 116 行 |

**请求参数：**

- `file`: 上传的文件（FormData 字段名）

**响应数据：**

```json
{
  "code": 200,
  "msg": "上传成功",
  "data": {
    "url": "https://example.com/uploads/2024/06/cover.jpg"
  }
}
```

**后端注意事项：**
- 前端限制：仅允许图片文件，大小不超过 2MB
- 返回的 `url` 需要是可直接访问的完整 URL
- 建议支持 jpg、png、gif、webp 格式

---

## P3 — 留言模块

> 留言墙页面当前完全使用 Mock 数据，优先级较低。

### 24. 获取留言列表 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `GET /api/comment/list` |
| 需要 Token | **是** |
| 前端源码 | `src/views/article/comment/index.vue`（当前使用 `@/mock/temp/commentList`） |

**响应数据：**

```typescript
interface CommentItem {
  id: number
  date: string           // 留言日期
  content: string        // 留言内容
  collection: number     // 收藏数
  comment: number        // 评论数
  userName: string       // 留言人昵称
}
```

```json
{
  "code": 200,
  "msg": "获取成功",
  "data": [
    {
      "id": 1,
      "date": "2024-09-03",
      "content": "加油！学好Node 自己写个小Demo",
      "collection": 5,
      "comment": 8,
      "userName": "匿名"
    }
  ]
}
```

---

### 25. 发表留言 (NEW)

| 项目 | 说明 |
|------|------|
| 路径 | `POST /api/comment` |
| 需要 Token | **是** |
| 前端源码 | `src/components/business/comment-widget/index.vue` |

**请求参数：**

```json
{
  "content": "很棒的项目！"
}
```

**响应数据：**

```json
{
  "code": 200,
  "msg": "留言成功",
  "data": null
}
```

---

## 待定接口（前端 UI 已预留，需确认业务需求）

以下接口前端页面有对应的 UI 入口，但功能较简单或业务逻辑未确定：

| 接口 | 前端位置 | 说明 |
|------|---------|------|
| `POST /api/auth/forget-password` | `src/views/auth/forget-password/index.vue` | 忘记密码，当前页面仅有用户名输入框，无验证码或邮箱验证流程。**建议确认重置密码的验证方式后再实现。** |
| `POST /api/auth/refresh` | — | Token 刷新接口。前端已预留 `refreshToken` 字段但未实现自动刷新逻辑。见 `backend-api-guide.md` 7.3 节。 |
| `DELETE /api/article/{id}` | — | 删除文章。前端文章列表页面暂无删除按钮，但后续可能需要。 |

---

## 数据库表设计建议

基于前端数据结构，后端至少需要以下数据表：

| 表名 | 说明 | 关键字段 |
|------|------|---------|
| `user` | 用户表 | id, userName, password, phone, gender, email, avatar, status, createTime |
| `role` | 角色表 | roleId, roleName, roleCode, description, enabled, createTime |
| `user_role` | 用户-角色关联表 | userId, roleId |
| `menu` | 菜单表 | id, parentId, name, path, component, icon, sort, type(menu/button), enabled, ... |
| `role_menu` | 角色-菜单关联表 | roleId, menuName |
| `article` | 文章表 | id, title, typeId, content, cover, visible, viewCount, createTime |
| `article_type` | 文章分类表 | id, name |
| `comment` | 留言表 | id, userId, content, collectionCount, commentCount, createTime |

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
第 4 步：实现 P2 文章模块
         文件上传 → 文章分类 → 文章 CRUD
         ↓
第 5 步：实现 P3 留言模块
         留言列表 → 发表留言
```

---

## 接口汇总表

| # | 方法 | 路径 | 说明 | 优先级 | 状态 |
|:-:|------|------|------|:------:|:----:|
| 1 | POST | `/api/auth/login` | 用户登录 | P0 | 已定义 |
| 2 | GET | `/api/user/info` | 获取用户信息 | P0 | 已定义 |
| 3 | POST | `/api/auth/register` | 用户注册 | P0 | **新增** |
| 4 | GET | `/api/v3/system/menus` | 获取菜单列表 | P0 | 已定义 |
| 5 | GET | `/api/user/list` | 用户列表（分页） | P1 | 已定义 |
| 6 | POST | `/api/user` | 新增用户 | P1 | **新增** |
| 7 | PUT | `/api/user/{id}` | 编辑用户 | P1 | **新增** |
| 8 | DELETE | `/api/user/{id}` | 删除用户 | P1 | **新增** |
| 9 | GET | `/api/role/list` | 角色列表（分页） | P1 | 已定义 |
| 10 | POST | `/api/role` | 新增角色 | P1 | **新增** |
| 11 | PUT | `/api/role/{roleId}` | 编辑角色 | P1 | **新增** |
| 12 | DELETE | `/api/role/{roleId}` | 删除角色 | P1 | **新增** |
| 13 | GET | `/api/role/{roleId}/permissions` | 获取角色权限 | P1 | **新增** |
| 14 | PUT | `/api/role/{roleId}/permissions` | 保存角色权限 | P1 | **新增** |
| 15 | POST | `/api/system/menu` | 新增菜单 | P1 | **新增** |
| 16 | PUT | `/api/system/menu/{id}` | 编辑菜单 | P1 | **新增** |
| 17 | DELETE | `/api/system/menu/{id}` | 删除菜单 | P1 | **新增** |
| 18 | GET | `/api/article/list` | 文章列表（分页） | P2 | **新增** |
| 19 | GET | `/api/article/{id}` | 文章详情 | P2 | **新增** |
| 20 | POST | `/api/article` | 发布文章 | P2 | **新增** |
| 21 | PUT | `/api/article/{id}` | 编辑文章 | P2 | **新增** |
| 22 | GET | `/api/article/types` | 文章分类列表 | P2 | **新增** |
| 23 | POST | `/api/common/upload` | 文件上传 | P2 | **新增** |
| 24 | GET | `/api/comment/list` | 留言列表 | P3 | **新增** |
| 25 | POST | `/api/comment` | 发表留言 | P3 | **新增** |

---

*文档版本：v1.0*
*更新日期：2026-02-26*
*适用前端版本：art-design-pro v3.0.1*