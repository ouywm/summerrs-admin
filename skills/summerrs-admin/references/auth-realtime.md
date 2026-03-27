# Auth And Realtime Patterns

This reference covers repo-specific auth, online-user, kick-out, and Socket.IO
patterns. These flows are cross-crate by design.

## Canonical Examples

- Auth route: `crates/summer-system/src/router/auth.rs`
- Auth service: `crates/summer-system/src/service/auth_service.rs`
- Online user service: `crates/summer-system/src/service/online_service.rs`
- Socket gateway plugin: `crates/summer-system/src/plugins/socket_gateway.rs`
- Socket event constants: `crates/summer-system/src/socketio/core/event.rs`
- Socket room rules: `crates/summer-system/src/socketio/core/room.rs`
- Socket connection service: `crates/summer-system/src/socketio/connection/service.rs`

## Auth Responsibility Split

### Foundation Lives In `summer-auth`

`crates/summer-auth` provides:

- access and refresh token handling
- session management
- online user tracking primitives
- extractors and middleware
- path-based auth helpers
- user type and device type support

### App-Facing Auth Behavior Lives In `summer-system`

`crates/summer-system/src/service/auth_service.rs` owns:

- admin login
- refresh token handling
- device/session views
- single-device kick-out
- role and permission assembly

So login work usually spans:

- `summer-auth`
- `AuthService`
- related DTOs and VOs
- login-log handling

## Route-Level Session Extraction

Common patterns in this repo:

- `AdminUser`
- `LoginUser`

Example:

```rust
pub async fn get_user_info(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<SysUserService>,
) -> ApiResult<Json<UserInfoVo>> {
    ...
}
```

## Online Users And Kick-Out

Main entry points:

- `crates/summer-system/src/service/online_service.rs`
- `crates/summer-system/src/service/sys_user_service.rs`
- `crates/summer-system/src/socketio/connection/service.rs`

### Important Rule

"Kick out" is not just session invalidation.

The standard flow is usually:

1. `SessionManager` performs `kick_out`, `logout`, or `force_refresh`
2. `SocketGatewayService` pushes the disconnect or severs sockets
3. socket session storage clears Redis indexes and session state

## Socket.IO Structure

### Entry Points

- plugin: `crates/summer-system/src/plugins/socket_gateway.rs`
- connection handling: `crates/summer-system/src/socketio/connection/*`
- shared socket definitions: `crates/summer-system/src/socketio/core/*`

### Core Responsibilities

- `core/event.rs`: event constants
- `core/model.rs`: payloads and socket state
- `core/room.rs`: room naming rules
- `core/emitter.rs`: shared outbound messaging
- `connection/service.rs`: auth, disconnect, disconnect-by-user
- `connection/session.rs`: Redis-backed socket session storage

## Room Rules

Current conventions:

- `user:{user_id}`
- `role:{role}`
- `all-{user_type}`

If you add realtime push behavior, prefer reusing these room rules instead of
inventing ad hoc room names in business code.

## Adding A New Socket Event

Recommended flow:

1. Add the event constant in `core/event.rs`
2. Add the payload shape in `core/model.rs`
3. Extend `core/emitter.rs` if the event should have a shared send path
4. Call the emitter or gateway from the relevant business service

## Verification

- Token or session changes: `cargo test -p summer-auth`
- System-level auth/realtime changes: `cargo test -p summer-system`
- For socket behavior, also run at least one manual login/connect/kick-out flow
