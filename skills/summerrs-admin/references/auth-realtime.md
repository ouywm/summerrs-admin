# Auth And Realtime Patterns

这一部分完全是仓库特有知识。登录态、在线用户、踢下线、Socket.IO 不是单点功能，而是跨 crate 联动。

## Canonical examples

- 认证 route：`crates/summer-system/src/router/auth.rs`
- 认证 service：`crates/summer-system/src/service/auth_service.rs`
- 在线用户与踢下线：`crates/summer-system/src/service/online_service.rs`
- Socket 网关插件：`crates/summer-system/src/plugins/socket_gateway.rs`
- 事件常量：`crates/summer-system/src/socketio/core/event.rs`
- Room 规则：`crates/summer-system/src/socketio/core/room.rs`

## 认证主线

### 基础能力在 `summer-auth`

`crates/summer-auth` 负责：

- access / refresh token
- session 管理
- online user 追踪
- extractor 与 middleware
- path auth
- user type / device type

### 应用层封装在 `AuthService`

`crates/summer-system/src/service/auth_service.rs` 负责：

- 管理员登录
- refresh token
- 登录设备列表
- 踢单设备
- 角色和权限装配

所以登录需求不要只改 router，通常会落到：

- `summer-auth`
- `AuthService`
- 相关 DTO / VO
- 登录日志 service

## Route 层登录态提取

本仓库常用：

- `AdminUser`
- `LoginUser`

例如：

```rust
pub async fn get_user_info(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<SysUserService>,
) -> ApiResult<Json<UserInfoVo>> {
    ...
}
```

## 在线用户与踢下线

主流程在：

- `crates/summer-system/src/service/online_service.rs`
- `crates/summer-system/src/service/sys_user_service.rs`
- `crates/summer-system/src/socketio/connection/service.rs`

### 关键规则

“踢下线”不是只做 token/session 失效。

标准流程通常是：

1. `SessionManager` 执行 `kick_out` / `logout` / `force_refresh`
2. `SocketGatewayService` 推送事件并断开 socket
3. `SocketSessionStore` 清理 Redis 索引与会话

## Socket.IO 结构

### 入口

- 插件：`crates/summer-system/src/plugins/socket_gateway.rs`
- 连接处理：`crates/summer-system/src/socketio/connection/*`
- 核心定义：`crates/summer-system/src/socketio/core/*`

### 核心职责

- `core/event.rs`：事件名常量
- `core/model.rs`：payload 和 socket state
- `core/room.rs`：room 命名规则
- `core/emitter.rs`：统一推送服务
- `connection/service.rs`：连接认证、断开、按用户断开
- `connection/session.rs`：Redis 中的 socket session 存储

## Room 规则

当前约定：

- `user:{user_id}`
- `role:{role}`
- `all-{user_type}`

如果新增推送能力，优先复用这些 room 规则，不要在业务代码里随手拼新字符串。

## 新增 Socket 事件怎么做

推荐步骤：

1. 在 `core/event.rs` 新增事件常量
2. 在 `core/model.rs` 定义 payload
3. 如需统一发消息，补 `core/emitter.rs`
4. 在业务 service 中调用 emitter 或 socket gateway

## 验证建议

- 改 token / session：`cargo test -p summer-auth`
- 改 system 联动：`cargo test -p summer-system`
- 如果是 socket 行为，至少手工走一遍登录、连接、踢下线链路
