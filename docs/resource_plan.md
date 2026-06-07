# 权限资源模型重构方案

## 背景

当前权限模型以 RBAC 为主：用户关联角色，角色关联 `sys.menu`。`sys.menu` 同时承载前端菜单和按钮权限，登录时根据用户角色查询 `menu_type = Button` 的 `auth_mark`，再写入 Access Token。接口侧通过 `#[has_perm]` / `#[has_perms]` 检查这些权限码。

这套模型能支撑页面和按钮级控制，但无法清晰表达后端 API 资源权限，尤其是一个前端操作依赖多个后端接口时。

典型场景：

- 用户拥有“新增用户”按钮权限。
- 前端打开新增用户弹窗时，需要请求角色下拉框、部门树、岗位列表等辅助接口。
- 这些辅助接口不是“角色管理列表”权限，也不应该要求用户拥有完整的角色管理权限。

因此需要把前端视角和后端视角拆开建模。

## 权限概念分层

### Menu：菜单

菜单表示前端页面或路由的可见性。

当前继续使用 `sys.menu(menu_type = Menu)` 表达菜单。

菜单回答的问题是：

- 用户能不能看到这个页面？
- 前端路由树里是否返回这个节点？

### Action：操作

操作表示用户能不能执行某个业务动作，也就是现在的按钮权限。

当前继续使用 `sys.menu(menu_type = Button)` 表达 Action，`auth_mark` 是操作权限码，例如：

- `system:user:list`
- `system:user:create`
- `system:user:update`
- `system:user:delete`

操作回答的问题是：

- 用户能不能看到某个按钮？
- 用户能不能执行某个业务动作？

### Resource：资源

资源表示后端 API 资源，通常由 HTTP Method + Path 唯一确定，例如：

- `POST /api/user`
- `GET /api/role/list`
- `GET /api/role/{role_id}/permissions`

资源回答的问题是：

- 当前请求命中了哪个后端 API 资源？
- 这个 API 是否需要资源权限检查？

### ActionResource：操作资源关系

一个前端操作通常需要一个或多个后端资源。

例如：

```text
Action: system:user:create
  -> POST /api/user
  -> GET /api/role/options
  -> GET /api/dept/tree
```

这表示用户拥有 `system:user:create` 时，不仅可以提交新增用户，也可以访问新增用户表单所需的辅助资源。

这比把 `GET /api/role/list` 直接绑定到 `system:role:list` 更准确。角色管理列表权限和新增用户表单里的角色下拉框权限不是同一件事。

## 推荐接口拆分原则

管理型列表接口和选择器接口应尽量拆开。

例如：

```text
GET /api/role/list      -> 角色管理页面使用，绑定 system:role:list
GET /api/role/options   -> 表单下拉框使用，可绑定 system:user:create 或其他操作
```

如果暂时复用 `GET /api/role/list`，也应通过 `ActionResource` 明确它可以被哪些 Action 依赖，而不是要求用户拥有角色管理权限。

## 第一阶段落地范围

第一阶段不推翻现有菜单权限，不新增独立 `sys.action` 表，而是复用现有按钮权限作为 Action。

新增：

- `sys.resource`：后端 API 资源表。
- `sys.action_resource`：Action 与 Resource 的多对多关系表。
- `ResourcePermissionPolicy`：启动时从数据库加载资源策略到内存。
- `ResourcePermissionStrategy`：在 JWT 鉴权成功后执行资源权限检查。

兼容策略：

- 未注册到 `sys.resource` 的接口继续放行，避免一次上线要求历史所有接口都补资源数据。
- 已注册但未启用的资源继续放行。
- 已注册且启用，但没有绑定任何 Action 的资源继续放行，视为资源配置未完成。
- 已注册、启用且绑定了 Action 的资源，必须命中用户当前权限列表中的至少一个 Action。

这样可以逐步把关键接口纳入后端资源权限，而不会破坏现有系统。

## 表结构设计

### sys.resource

字段：

- `id`：主键。
- `resource_name`：资源名称。
- `resource_code`：资源编码，稳定唯一，例如 `system:user:create.submit`。
- `method`：HTTP 方法，统一大写，例如 `GET`、`POST`。
- `path`：API 路径，支持 `{param}` 参数形式，例如 `/api/user/{id}`。
- `description`：说明。
- `enabled`：是否启用资源权限。
- `create_time` / `update_time`：时间戳。

唯一约束：

- `resource_code`
- `(method, path)`

### sys.action_resource

字段：

- `id`：主键。
- `action_menu_id`：Action ID，对应 `sys.menu.id`，且应为 `menu_type = Button`。
- `resource_id`：Resource ID，对应 `sys.resource.id`。

唯一约束：

- `(action_menu_id, resource_id)`

## 运行时授权流程

1. 请求进入 system 路由。
2. `JwtStrategy` 校验 Access Token，成功后注入 `UserSession`。
3. `ResourcePermissionStrategy` 获取 method/path。
4. 策略查找是否命中已启用资源。
5. 如果没有命中资源，放行。
6. 如果命中资源但资源未绑定 Action，放行。
7. 如果命中资源并绑定 Action，检查 `UserSession.profile.permissions` 是否匹配任一 Action 的 `auth_mark`。
8. 匹配成功放行，否则返回 403。

权限匹配继续复用 `summer_auth::permission_matches`，因此支持 `system:*`、`system:user:*` 等通配权限。

## Token 与缓存策略

Access Token 继续只保存角色和 Action 权限。

不建议第一阶段把所有 Resource 权限塞进 JWT。原因：

- Resource 数量可能明显多于按钮权限。
- Resource 和 Action 的映射属于服务端策略，适合用内存缓存。
- 权限变更后现有 `force_refresh` 机制已经能让用户刷新 Action 权限。

资源策略变更后需要刷新服务端内存策略；后续可以增加资源管理接口，在保存资源映射后热更新 `ResourcePermissionPolicy`。

## 后续阶段

第二阶段可以增加：

- 资源管理页面。
- Action 资源绑定页面。
- 从 OpenAPI 或 route inventory 自动生成资源草稿。
- `GET /role/options`、`GET /dept/options` 等窄接口，减少管理接口复用。
- 独立 `sys.action` 表，把 Action 从 `sys.menu(menu_type=Button)` 中拆出。

第三阶段再考虑：

- 资源组。
- 数据范围权限。
- 用户级权限覆盖。
- 多租户资源隔离策略。
