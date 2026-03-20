# Socket.IO 前端测试说明

## 目的

这个文档给前端测试页使用。

当前后端 Socket.IO 只提供：

- 连接鉴权
- 断开连接后的清理
- 用户被禁用或会话被撤销时的强制断开

目前还没有业务事件。
所以第一版前端测试页只需要验证：连接状态、鉴权、断开、重连。

## 当前后端配置

开发环境当前配置：

- 后端地址：`http://localhost:8080`
- 全局前缀：`/api`
- Socket.IO 路径：`/api/socket.io`
- 命名空间：`/summer-admin`

配置来源：

- `config/app-dev.toml`

## 前端连接示例

连接时只使用 `accessToken`。
不要把 `refreshToken` 传给 Socket.IO。

```ts
import { io, type Socket } from "socket.io-client";

export function createAdminSocket(accessToken: string): Socket {
  return io("http://localhost:8080/summer-admin", {
    path: "/api/socket.io",
    auth: {
      accessToken
    }
  });
}
```

## 重要约定

- auth 字段名必须是 `accessToken`
- namespace 必须是 `/summer-admin`
- request path 必须是 `/api/socket.io`
- 前端本地开发时和后端是跨域的，所以连接地址要写后端地址 `http://localhost:8080`

错误示例：

- 把 `http://localhost:8080/api/socket.io` 当成 namespace 地址
- 没有传 `auth.accessToken`
- 连接时传 `refreshToken`

## 建议的测试页功能

第一版测试页可以很简单：

- 输入或读取当前 `accessToken`
- 点击连接
- 点击断开
- 显示 `socket.id`
- 显示当前状态：`connecting / connected / disconnected / error`
- 打印事件日志
- 支持用最新 `accessToken` 重新连接

## 前端建议监听的事件

```ts
socket.on("connect", () => {
  console.log("socket connected", socket.id);
});

socket.on("disconnect", (reason) => {
  console.log("socket disconnected", reason);
});

socket.on("connect_error", (err) => {
  console.error("socket connect error", err.message);
});
```

## 当前预期行为

### 1. 正常连接

如果 `accessToken` 有效：

- 连接成功
- 后端日志出现 `Socket.IO connected`
- 前端收到 `connect`

### 2. 缺少 token

如果 `auth.accessToken` 缺失或为空：

- 连接失败
- 前端收到 `connect_error`
- 预期提示：

```txt
Missing accessToken in Socket.IO auth payload
```

### 3. token 无效或过期

如果 token 无效、过期、已被禁用：

- 连接失败
- 前端收到 `connect_error`
- 错误消息来自后端鉴权逻辑

### 4. 当前用户被管理员禁用

如果后端把当前用户禁用了：

- 后端会执行 `ban_user`
- 后端会执行 `logout_all`
- 后端会主动断开当前用户全部 Socket
- 前端应收到 `disconnect`

这时候前端建议这样处理：

1. 先尝试 HTTP `/auth/refresh`
2. refresh 成功后，用新的 `accessToken` 重新连接
3. refresh 失败则清理登录态并跳转登录页

## 当前后端事件范围

现在后端还没有主动发送自定义业务事件。

所以这个测试页不要默认假设存在下面这些事件：

- `message`
- `notice`
- `chat`
- `system_ready`

如果后面要测试服务端主动推送，再单独补一个 demo 事件即可。

## 前端开发建议

建议拆分为：

- `src/utils/socket.ts`：连接工厂、断开、重连逻辑
- `src/views/.../socket-io-test/index.vue`：测试页面

`socket.ts` 可以先暴露：

- `connectSocket(accessToken)`
- `disconnectSocket()`
- `reconnectSocket(accessToken)`

## 手工测试清单

### 场景 1：有效 token

- 打开页面
- 使用当前 token 连接
- 看到 connected 状态

### 场景 2：不传 token

- 清空 token
- 点击连接
- 看到 `connect_error`

### 场景 3：错误 token

- 使用伪造 token
- 点击连接
- 看到 `connect_error`

### 场景 4：管理员禁用当前用户

- 保持当前 socket 已连接
- 在后台禁用当前用户
- 验证前端收到断开事件

---

后面如果补了业务事件，也建议继续在这个测试页基础上扩展，而不是替换掉连接/鉴权测试。
