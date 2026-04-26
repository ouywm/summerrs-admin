# OpenAI OAuth 托管账号设计

> 日期：2026-04-26
> 状态：设计已确认，待进入实施计划
> 范围：`summer-ai` 首期 OpenAI OAuth 托管账号闭环

## 1. 背景

当前 `summer-ai` 的 `channel_account` 已支持 `credential_type = "oauth"` 的数据形态，但 relay 运行时仍未真正支持 OAuth 账号；`channel_store` 在构造 `ServiceTarget` 时会直接返回 `NotImplemented("oauth credentials")`。

参考项目 `docs/relay/go/sub2api-main` 已经把 OpenAI / Gemini / Claude / Antigravity 的 OAuth 授权、refresh、热路径取 token、后台刷新拆成多层实现。首期不要求把这几家一次性做完，但要求 OpenAI OAuth 能形成完整闭环，并且实现形态要兼容后续继续扩展其他 provider。

本次设计目标是：

- 首期完成 OpenAI OAuth 托管账号闭环
- admin 能发起授权、换取 token、创建或更新托管账号
- relay 能在热路径使用 OAuth 账号，并在 access token 临近过期时自动刷新
- 不新增 `channel_account` 的 OpenAI 专用字段
- 首期授权会话使用内存 `SessionStore`

## 2. 目标与非目标

### 2.1 目标

- 支持 OpenAI PKCE Authorization Code Flow
- 支持通过 admin 业务动作创建 OpenAI OAuth 托管账号
- `channel_account.credentials` 存储 OpenAI OAuth 凭证及账号元信息
- relay 热路径支持 `credential_type = "oauth"` 的 OpenAI 账号
- 支持 refresh token 自动刷新
- 刷新成功后落库，并更新运行时使用的 access token
- 失败时给出明确错误，并保留后续扩展到 Redis / DB session 的缝

### 2.2 非目标

- 首期不实现多 provider OAuth 框架全量接入
- 首期不实现多实例 admin / 重启后继续回调
- 首期不新增 pending session 持久化表
- 首期不实现账号级代理 OAuth
- 首期不做 OpenAI WebSocket / passthrough / privacy 额外能力
- 首期不做后台定时批量刷新任务

## 3. 现状与约束

### 3.1 已有基础

- `channel_account` 已有：
  - `credential_type`
  - `credentials`
  - `extra`
  - `expires_at`
  - `status`
  - `schedulable`
- `summer-ai-admin` 已有 `channel_account` CRUD 骨架
- `summer-ai-relay` 已有基于 `channel + channel_account` 的路由和 `ServiceTarget` 构造
- `summer-ai-core` 已有 `AuthData`、`ServiceTarget`、adapter dispatcher

### 3.2 当前阻塞点

- relay 运行时未实现 OAuth token 解析与刷新
- `build_service_target(...)` 是同步函数，不适合直接内嵌 async refresh 流程
- admin 目前只有通用 CRUD，没有 OpenAI OAuth 的业务动作接口

## 4. 数据模型设计

### 4.1 表结构结论

首期 **不修改** `ai.channel_account` 表结构。

理由：

- 参考项目 OpenAI OAuth 的核心信息也主要落在 `credentials`
- 现有表已足够表达 `oauth` 账号
- 首期只做 OpenAI，不需要新增 `oauth_provider_type` 专用列

### 4.2 `credential_type`

OpenAI OAuth 账号约定：

- `credential_type = "oauth"`

### 4.3 `credentials` 结构

OpenAI OAuth 账号的 `channel_account.credentials` 统一为扁平 JSON：

```json
{
  "access_token": "access_token_value",
  "refresh_token": "refresh_token_value",
  "id_token": "id_token_value",
  "expires_at": "2026-04-26T18:00:00Z",
  "client_id": "app_xxx",
  "email": "user@example.com",
  "chatgpt_account_id": "acc_xxx",
  "chatgpt_user_id": "user_xxx",
  "organization_id": "org_xxx",
  "plan_type": "plus",
  "subscription_expires_at": "2026-05-01T00:00:00Z",
  "_token_version": 1777200000000
}
```

说明：

- `expires_at` 使用 RFC3339 时间字符串
- `_token_version` 为内部并发保护字段，用于 token 刷新后的版本比较
- 首期不嵌套 `oauth: {...}`，保持与当前 `summer-ai` 文档示例和参考项目运行时 helper 的读取习惯一致

### 4.4 `extra` 结构

首期 `extra` 不承载核心 OAuth 凭证，仅预留后续账号行为开关。

首期可能使用到的最小字段：

```json
{
  "oauth_provider": "openai"
}
```

该字段不是强制要求，但建议写入，便于后续查询和扩展。

## 5. 模块与分层设计

参考项目的经验不是先抽一个“大而全”的 `OAuthProvider`，而是拆成几层：

- provider-specific OAuth service
- `SessionStore`
- `TokenRefresher`
- `OAuthRefreshAPI`
- runtime `TokenProvider`

Rust 版对应落点如下。

### 5.1 `summer-ai-core`

新增模块：

```text
crates/summer-ai/core/src/oauth/
├── mod.rs
├── session_store.rs
├── openai/
│   ├── mod.rs
│   ├── pkce.rs
│   ├── session.rs
│   ├── types.rs
│   ├── codec.rs
│   └── client.rs
```

职责：

- 纯 OpenAI OAuth 基础能力
- PKCE `state` / `code_verifier` / `code_challenge`
- 授权 URL 构建
- OpenAI OAuth session payload
- 通用内存 `SessionStore<S>`
- OpenAI token response / ID token claims / credential codec

不负责：

- DB 落库
- account CRUD
- runtime refresh 编排

### 5.2 `summer-ai-admin`

新增：

```text
crates/summer-ai/admin/src/router/openai_oauth.rs
crates/summer-ai/admin/src/service/openai_oauth_service.rs
```

职责：

- 生成授权 URL
- 换取 token
- 创建 OpenAI OAuth 托管账号
- 更新已有 OpenAI OAuth 托管账号

### 5.3 `summer-ai-relay`

新增：

```text
crates/summer-ai/relay/src/service/oauth/
├── mod.rs
├── token_refresher.rs
├── refresh_api.rs
├── openai_token_provider.rs
└── credentials.rs
```

职责：

- 读取 OpenAI OAuth credentials
- 判断是否需要刷新
- 串行化 refresh 流程
- 落库更新 credentials
- 在热路径返回可用 access token

### 5.4 `summer-ai-model`

保持表结构不变，仅补充：

- DTO 校验增强
- VO 展示字段约定

## 6. 核心抽象设计

### 6.1 `SessionStore<S>`

参考项目每个 provider 都有自己的内存 store，但模式一致。

Rust 版统一抽一个泛型内存 store：

```rust
pub struct SessionStore<S> {
    inner: Arc<RwLock<HashMap<String, StoredSession<S>>>>,
    ttl: Duration,
}
```

能力：

- `set(session_id, payload)`
- `get(session_id) -> Option<S>`
- `delete(session_id)`
- 定时清理过期 session

OpenAI session payload：

```rust
pub struct OpenAiOAuthSession {
    pub state: String,
    pub code_verifier: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
}
```

首期只在 admin 进程内使用。

### 6.2 `CredentialCodec`

参考项目没有显式命名这个接口，但等价职责存在于：

- `BuildAccountCredentials(...)`
- `GetCredential(...)`
- `GetCredentialAsTime(...)`

Rust 版建议明确为 codec：

```rust
pub trait CredentialCodec<T> {
    fn encode(&self, value: &T) -> serde_json::Value;
    fn decode(&self, value: &serde_json::Value) -> Result<T, CodecError>;
}
```

OpenAI codec 负责：

- `OpenAiTokenInfo -> serde_json::Value`
- `serde_json::Value -> OpenAiStoredCredentials`

### 6.3 `TokenRefresher`

参考项目的抽法直接借用：

```rust
#[async_trait]
pub trait TokenRefresher {
    fn can_refresh(&self, channel: &channel::Model, account: &channel_account::Model) -> bool;
    fn needs_refresh(&self, account: &channel_account::Model, refresh_window: Duration) -> bool;
    async fn refresh(
        &self,
        account: &channel_account::Model,
    ) -> ApiResult<serde_json::Value>;
}
```

首期实现：

- `OpenAiTokenRefresher`

### 6.4 `OAuthRefreshApi`

参考项目真正的公共核心在 `OAuthRefreshAPI`。

Rust 版职责保持一致：

- 进程内互斥，避免同账号并发 refresh
- 刷新前 DB 重读，避免旧 refresh token 竞争
- 二次检查是否仍需要刷新
- 调具体 `TokenRefresher`
- 更新 `_token_version`
- 落库保存新的 credentials

首期不做：

- Redis 分布式锁
- refresh race 恢复的复杂降级逻辑

但结构要预留扩展口。

### 6.5 `OpenAiTokenProvider`

职责：

- 从账号 credentials 读取 access token
- 判断是否临近过期
- 必要时调用 `OAuthRefreshApi`
- 返回可用 `AuthData::Single`

## 7. Admin 业务接口设计

首期不靠纯 CRUD 完成 OAuth 建号，而是新增业务动作接口。

### 7.1 生成授权 URL

`POST /openai-oauth/auth-url`

请求：

```json
{
  "redirectUri": "http://localhost:1455/auth/callback"
}
```

响应：

```json
{
  "authUrl": "https://auth.openai.com/oauth/authorize?...",
  "sessionId": "session_xxx"
}
```

行为：

- 生成 `state`
- 生成 `code_verifier`
- 生成 `code_challenge`
- 写入内存 `SessionStore`
- 返回 auth URL 和 sessionId

### 7.2 换取 token 并创建托管账号

`POST /openai-oauth/exchange`

请求：

```json
{
  "sessionId": "session_xxx",
  "code": "oauth_code",
  "state": "oauth_state",
  "channelId": 1,
  "name": "OpenAI OAuth 账号 A",
  "remark": "托管账号",
  "testModel": "gpt-4.1"
}
```

响应：

```json
{
  "accountId": 123,
  "created": true
}
```

行为：

- 从 `SessionStore` 读取 session
- 校验 `state`
- 用 `code + code_verifier + redirect_uri + client_id` 调 OpenAI token endpoint
- 解析 `id_token` / access token claims
- 生成标准化 credentials
- 创建 `channel_account`

### 7.3 刷新已有 OAuth 账号

`POST /openai-oauth/{id}/refresh`

行为：

- 只允许对 OpenAI + `credential_type = oauth` 的账号调用
- 调用 `OpenAiTokenRefresher`
- 成功后落库
- 返回最新摘要信息

### 7.4 约束校验

- `channel_id` 必须存在
- 关联的 channel 必须是 OpenAI 类型
- 非 OpenAI channel 不允许创建 OpenAI OAuth 账号
- `credential_type` 固定为 `oauth`

## 8. Relay 运行时数据流

### 8.1 当前问题

当前 `build_service_target(...)` 是同步函数，而 OAuth token 获取需要 async refresh。

### 8.2 调整方案

把当前流程拆成两步：

1. `resolve_auth_data(...).await`
2. `build_service_target_with_auth(...)`

建议新增：

```rust
pub async fn resolve_auth_data(
    channel: &channel::Model,
    account: &channel_account::Model,
    selected_key: &str,
    scope: EndpointScope,
    openai_provider: &OpenAiTokenProvider,
) -> Result<AuthData, RelayError>;

pub fn build_service_target_with_auth(
    channel: &channel::Model,
    account: &channel_account::Model,
    auth: AuthData,
    logical_model: &str,
    scope: EndpointScope,
) -> Result<ServiceTarget, RelayError>;
```

### 8.3 热路径逻辑

- API key 账号：
  - 保持现有逻辑
- OpenAI OAuth 账号：
  - 解码 credentials
  - 若未过期，直接返回 `AuthData::Single(access_token)`
  - 若临近过期，执行 refresh
  - refresh 成功后写回 DB
  - 返回新的 `AuthData::Single(access_token)`

### 8.4 生效范围

首期仅对：

- `channel.channel_type = OpenAI`
- `channel_account.credential_type = oauth`

启用 OpenAI OAuth provider。

其他 provider 保持现状不变。

## 9. OpenAI OAuth HTTP 交互

### 9.1 Generate Auth URL

- `response_type=code`
- `client_id`
- `redirect_uri`
- `scope=openid profile email offline_access`
- `state`
- `code_challenge`
- `code_challenge_method=S256`
- `id_token_add_organizations=true`
- `codex_cli_simplified_flow=true`

### 9.2 Exchange Code

请求 token endpoint：

- `grant_type=authorization_code`
- `client_id`
- `code`
- `redirect_uri`
- `code_verifier`

### 9.3 Refresh Token

请求 token endpoint：

- `grant_type=refresh_token`
- `client_id`
- `refresh_token`
- `scope=openid profile email`

## 10. 错误处理

### 10.1 Admin 授权阶段

- session 不存在或过期
- `state` 不匹配
- code exchange 失败
- OpenAI token response 缺失关键字段
- channel 不存在或不是 OpenAI

### 10.2 Relay 热路径

- credentials 解码失败
- `refresh_token` 缺失且 access token 已不可用
- refresh 失败
- DB 更新失败

### 10.3 错误策略

首期策略偏保守：

- admin 接口失败直接返回错误
- relay 热路径 refresh 失败时直接返回错误，不静默回退到过期 token
- 不自动修改账号状态；状态自动治理留给后续阶段

## 11. 测试设计

### 11.1 `summer-ai-core`

- PKCE 参数生成测试
- auth URL 构建测试
- session store TTL 测试
- OpenAI credential codec encode/decode 测试
- ID token / claims 解析测试

### 11.2 `summer-ai-admin`

- 生成 auth URL 测试
- exchange code 成功创建账号测试
- exchange code 失败测试
- 非 OpenAI channel 拒绝测试
- refresh 指定账号测试

### 11.3 `summer-ai-relay`

- API key 账号路径不回归
- OpenAI OAuth 账号未过期时直接取 token
- OpenAI OAuth 账号临近过期时触发 refresh
- refresh 成功后 `build_service_target` 使用新 token
- credentials 非法时返回明确错误

## 12. 实施顺序

1. `core/oauth` 基础模块
2. `admin/openai_oauth_service + router`
3. `relay/service/oauth` 运行时刷新链路
4. `pipeline/channel_store` 接入 async auth resolve
5. 单测与集成测试

## 13. 后续扩展点

- 把 `SessionStore` 从内存切到 Redis / DB
- 增加 `oauth_provider_type`
- 继续接 Gemini / Claude / Antigravity
- 增加后台 `TokenRefreshService`
- 增加分布式锁与 refresh race 恢复

## 14. 决策摘要

- 首期只做 OpenAI OAuth
- 首期 session 走内存
- 不新增 `channel_account` OpenAI 专用列
- 参考项目的分层思路采用：
  - provider-specific OAuth service
  - `SessionStore`
  - `TokenRefresher`
  - `OAuthRefreshApi`
  - runtime `TokenProvider`
- 不先抽一个过重的全局 `OAuthProvider` 大接口
