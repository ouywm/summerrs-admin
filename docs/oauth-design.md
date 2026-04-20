# OAuth 支持设计文档

> **目标读者**：summer-ai 后端 / 前端开发者
> **状态**：设计稿（未实施）
> **参考实现**：`docs/relay/go/axonhub/llm/oauth/`

---

## 一、为什么要做

多家 LLM 厂商的新 CLI 工具走 OAuth，不接受纯 API Key：

| 厂商 | 工具 | 认证方式 |
|---|---|---|
| GitHub | Copilot Chat | OAuth Device Flow + token 二次交换 |
| OpenAI | Codex CLI | OAuth Device Flow |
| Anthropic | Claude Code | PKCE Authorization Code Flow |
| Google | Antigravity | OAuth Device Flow |

要让 relay 能代理这些流量，`channel_account` 必须能存 OAuth credentials、自动 refresh、并在每次请求时吐出有效的 access_token。

本轮设计参考 axonhub 的 Go 实现（`llm/oauth/`），用 Rust 重写，对齐当前 summer-ai 的 `channel + channel_account` 两级表架构。

---

## 二、目标 / 非目标

### 目标

- `channel_account.credential_type = "oauth"` 能工作
- 存储 access_token / refresh_token / expires_at，自动 refresh
- 支持三种接入：**Device Flow / PKCE / 粘贴 JSON**
- 支持 **Token 二次交换**（GitHub OAuth token → Copilot 专用 token）
- refresh 失败自动把 account 转 `Status::Expired`，并告警

### 非目标（本轮不做）

- 租户级 OAuth 应用管理（不同租户用不同 `client_id`）
- Credentials 落库加密（本轮明文 JSONB；生产用 Vault/KMS 包一层）
- 多实例 refresh 广播（Redis pub/sub 通知其他 relay 实例 reload）
- OAuth 授权给我们的客户端（我们反向扮演 IdP）
- 审计日志

---

## 三、数据模型

### 3.1 `channel_account.credential_type`

字段已存在，本轮使用 `"api_key"` 和 `"oauth"` 两个值。其他（`cookie` / `session` / `token`）留给后续。

### 3.2 `channel_account.credentials` JSONB

按 `credential_type` 分派形态：

```jsonc
// api_key（P5 已实现）
{
  "api_keys": ["sk-1", "sk-2"],
  "api_key":  "sk-legacy"    // 遗留兼容字段
}

// oauth（本轮新增）
{
  "oauth": {
    "client_id":     "Iv1.b507a08c87ecfe98",
    "access_token":  "gho_xxx...",
    "refresh_token": "ghr_xxx...",
    "id_token":      "",
    "expires_at":    "2026-04-20T12:34:56Z",
    "token_type":    "Bearer",
    "scopes":        ["read:user"],
    "exchange": {              // 可选：token 二次交换缓存（Copilot 用）
      "token":      "...",
      "expires_at": "2026-04-20T12:04:56Z"
    }
  }
}
```

### 3.3 新字段

`ai.channel_account` 加一列：

```sql
ALTER TABLE ai.channel_account
  ADD COLUMN oauth_provider_type VARCHAR(32) NOT NULL DEFAULT '';
```

取值枚举（常量集中在 `core/src/oauth/configs.rs`）：

| 值 | 含义 | Flow 类型 |
|---|---|---|
| `""` | 不是 oauth | — |
| `github_device` | GitHub Copilot | DeviceFlow |
| `codex_device` | OpenAI Codex | DeviceFlow |
| `google_device` | Antigravity | DeviceFlow |
| `claude_pkce` | Claude Code | PkceFlow |
| `pasted` | 用户粘贴现成 JSON | 无 flow，只自动 refresh |

---

## 四、Rust 抽象

### 4.1 模块布局

```
crates/summer-ai/core/src/oauth/
├── mod.rs                 # 出口
├── credentials.rs         # OAuthCredentials struct + is_expired + parse_json
├── error.rs               # OAuthError 枚举
├── provider.rs            # OAuthProvider trait + 3 个实现
├── device_flow.rs         # DeviceFlowProvider
├── pkce_flow.rs           # PkceFlowProvider
├── pasted.rs              # PastedCredsProvider
├── exchange.rs            # TokenExchanger trait + CopilotExchanger
├── strategy.rs            # ExchangeStrategy (FormEncoded / Json)
└── configs.rs             # 各 provider 硬编码常量（URL / scope）
```

放在 `summer-ai-core` 的理由：`adapter/` 下的 adapter 实现可能需要直接用 OAuth token（比如 Claude adapter 走 Claude Code 凭证），协议层复用。relay 拿出来只是 resolve。

### 4.2 `OAuthCredentials`

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OAuthCredentials {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id_token: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<ExchangeCache>,
}

impl OAuthCredentials {
    /// 提前 3 分钟视为过期（对齐 axonhub 的 safety margin）。
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .map(|t| now + Duration::minutes(3) >= t)
            .unwrap_or(true)
    }

    /// 从粘贴的 JSON（如 `~/.claude/credentials.json` 内容）解析。
    pub fn parse_credentials_json(raw: &str) -> Result<Self, OAuthError>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExchangeCache {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
```

### 4.3 `OAuthProvider` trait

```rust
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// 返回当前有效的 access token（必要时自动 refresh + exchange）。
    async fn get_token(&self, http: &reqwest::Client) -> Result<String, OAuthError>;

    /// 返回当前 credentials snapshot（给持久化 / 展示 UI 用）。
    fn current_credentials(&self) -> OAuthCredentials;

    /// provider 类型标识，用于前端 / 日志。
    fn provider_type(&self) -> &'static str;
}
```

### 4.4 三种 Provider 实现

| 结构 | 用途 | 核心字段 |
|---|---|---|
| `DeviceFlowProvider` | RFC 8628 device flow | `config: DeviceFlowConfig`, `creds: RwLock<Creds>`, `exchanger: Option<Box<dyn TokenExchanger>>`, `singleflight: Notify`, `on_refreshed: Arc<dyn Fn>` |
| `PkceFlowProvider` | PKCE authorization code | `config: PkceConfig`, 其余同上 |
| `PastedCredsProvider` | 静态 JSON，仅 refresh | `creds: RwLock<Creds>`, `refresh_url: String`, `strategy: ExchangeStrategy`, `on_refreshed` |

三者共享的内部逻辑：

```rust
async fn get_access_token_with_refresh(&self, http: &reqwest::Client) -> Result<String, OAuthError> {
    {
        let guard = self.creds.read().await;
        if !guard.is_expired(Utc::now()) {
            return Ok(guard.access_token.clone());
        }
    }
    // 并发 refresh 合并（tokio::sync::Notify 或 tokio::sync::Mutex<Option<Shared<future>>>）
    let fresh = self.refresh_singleflight(http).await?;
    // 持久化回调
    (self.on_refreshed)(fresh.clone()).await;
    *self.creds.write().await = fresh.clone();
    Ok(fresh.access_token)
}
```

### 4.5 `TokenExchanger`（Copilot）

```rust
#[async_trait]
pub trait TokenExchanger: Send + Sync {
    async fn exchange(
        &self,
        http: &reqwest::Client,
        access_token: &str,
    ) -> Result<(String, DateTime<Utc>), OAuthError>;
}

pub struct CopilotExchanger;

#[async_trait]
impl TokenExchanger for CopilotExchanger {
    async fn exchange(&self, http: &reqwest::Client, access_token: &str)
        -> Result<(String, DateTime<Utc>), OAuthError> {
        // GET https://api.github.com/copilot_internal/v2/token
        //   Authorization: token {access_token}
        // 返回 {token, expires_at}
        ...
    }
}
```

Provider 的 `get_token` 内部调用链：

```
get_token
 ├─ exchange 启用？
 │   ├─ 是 → 先 get_access_token_with_refresh 拿 OAuth token
 │   │       └─ 再调 exchanger.exchange(...)
 │   │           └─ 缓存到 creds.exchange，命中直接返
 │   └─ 否 → 直接返 access_token
```

### 4.6 `ExchangeStrategy`

OAuth token 请求有两种 body 格式：

- **FormEncoded**（RFC 标准，GitHub / Codex 用）：`grant_type=refresh_token&client_id=...&refresh_token=...`
- **Json**（Anthropic 用）：`{"grant_type":"refresh_token","client_id":"...","refresh_token":"..."}`

```rust
#[async_trait]
pub trait ExchangeStrategy: Send + Sync {
    async fn refresh(
        &self,
        http: &reqwest::Client,
        token_url: &str,
        creds: &OAuthCredentials,
    ) -> Result<OAuthCredentials, OAuthError>;

    async fn exchange_code(
        &self,
        http: &reqwest::Client,
        token_url: &str,
        params: &ExchangeCodeParams,
    ) -> Result<OAuthCredentials, OAuthError>;
}

pub struct FormEncodedStrategy;
pub struct JsonStrategy;
```

### 4.7 Provider Configs（硬编码常量）

```rust
// core/src/oauth/configs.rs

pub const GITHUB_DEVICE: DeviceFlowConfig = DeviceFlowConfig {
    device_auth_url: "https://github.com/login/device/code",
    token_url: "https://github.com/login/oauth/access_token",
    client_id: "Iv1.b507a08c87ecfe98",           // GitHub Copilot 公开 client_id
    scopes: &["read:user"],
    user_agent: "summer-ai-relay/0.1",
    strategy: StrategyKind::FormEncoded,
};

pub const CLAUDE_PKCE: PkceConfig = PkceConfig {
    authorize_url: "https://claude.ai/oauth/authorize",
    token_url: "https://claude.ai/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",  // Claude Code 公开 client_id
    redirect_uri: "https://console.anthropic.com/oauth/code/callback",
    scopes: &["org:create_api_key", "user:profile", "user:inference"],
    strategy: StrategyKind::Json,
};

pub const COPILOT_EXCHANGER: CopilotExchangerConfig = ...;
```

---

## 五、三种 Flow 完整流程

### 5.1 Device Flow（GitHub / Codex / Google）

```
┌──────────┐                     ┌──────────┐                    ┌────────┐
│ Frontend │                     │  Backend │                    │ GitHub │
└────┬─────┘                     └────┬─────┘                    └────┬───┘
     │ POST /oauth/device/start        │                               │
     │   {provider: "github_device"}   │                               │
     │────────────────────────────────>│                               │
     │                                 │ POST /login/device/code       │
     │                                 │──────────────────────────────>│
     │                                 │<─ {device_code, user_code,   │
     │                                 │     verification_uri,         │
     │                                 │     expires_in, interval}     │
     │<─ {session_id, user_code,       │                               │
     │    verification_uri, ...}       │                               │
     │                                 │                               │
     │ 显示 user_code 给用户             │                               │
     │ 用户打开 verification_uri 授权    │                               │
     │                                 │                               │
     │ 每 interval 秒轮询:              │                               │
     │ POST /oauth/device/poll         │                               │
     │   {session_id}                  │                               │
     │────────────────────────────────>│                               │
     │                                 │ POST /login/oauth/access_token│
     │                                 │──────────────────────────────>│
     │                                 │<─ authorization_pending       │
     │<─ {status: "pending"}           │                               │
     │ ...                             │                               │
     │ ...                             │ 最终成功                        │
     │                                 │<─ {access_token, refresh_token│
     │                                 │    expires_in}                │
     │                                 │ 写入 channel_account           │
     │<─ {status: "ok", account_id}   │                               │
```

**Backend 状态机**（`DeviceFlowSession` 存 Redis，TTL = expires_in）：

```rust
pub struct DeviceFlowSession {
    pub session_id: String,          // 前端轮询用，不暴露 device_code
    pub provider_type: String,       // "github_device" 等
    pub device_code: String,         // 只在 backend 里
    pub user_code: String,
    pub verification_uri: String,
    pub interval_secs: u32,
    pub expires_at: DateTime<Utc>,
    pub channel_id: i64,             // 创建成功后要挂到哪个 channel
    pub account_name: String,        // 前端填的名字
}
```

### 5.2 PKCE Flow（Claude Code）

PKCE 比 device flow 简单一点，但需要前端开一个监听本地端口的"回调接收器"。**服务器部署环境**一般用不了（用户没本地浏览器），所以我们做两种 UI：

**UI-A：浏览器重定向**（部署在可公网访问的管理后台）

```
用户点"连接 Claude" → Backend 返回 authorize_url（含 state / code_challenge）
用户浏览器打开 authorize_url → Anthropic 授权 → 跳回我们的 /oauth/pkce/callback?code=...&state=...
→ Backend 用 code_verifier 换 token → 存 DB → 重定向回管理页
```

**UI-B：粘贴 code**（CLI 风格，无需 redirect 端点）

```
Backend 返回 authorize_url + code_verifier_ref
用户手动打开 URL 授权 → Anthropic 页面显示 code
用户把 code 粘回 UI → POST /oauth/pkce/exchange {code, code_verifier_ref}
```

本轮优先实现 UI-B（不需要 redirect URI 注册，部署灵活）。

### 5.3 粘贴 JSON

用户从 `~/.claude/credentials.json` / `gh auth status --show-token` 拿到现成的 access_token + refresh_token，直接粘贴：

```json
{
  "access_token": "sk-ant-oat01-...",
  "refresh_token": "sk-ant-ort01-...",
  "expires_at": "2026-04-20T...",
  "scopes": ["user:inference"]
}
```

后端走 `OAuthCredentials::parse_credentials_json`，写入 `channel_account.credentials.oauth`。后续 refresh 走 `PastedCredsProvider`（根据 `oauth_provider_type` 字段找到 token_url / strategy）。

---

## 六、自动刷新

### 6.1 Refresh 触发时机

**懒惰 refresh**（每次 `get_token` 检查 expires_at，过期才 refresh）：简单，但流量尖峰时多个并发请求可能触发 refresh storm —— 用 singleflight 解决。

**主动 refresh**（后台 task 定时检查 + 提前 refresh）：减少用户请求延迟。

本设计用**两者结合**：

1. `relay` 启动时 spawn 一个 refresh loop：每分钟扫描所有 OAuth account，把 `expires_at - now < 5min` 的提前 refresh。
2. `get_token` 内部仍然做 lazy check 兜底。

### 6.2 Refresh loop

```rust
// crates/summer-ai/relay/src/oauth/refresh_loop.rs

pub struct OAuthRefreshLoop {
    db: DbConn,
    redis: Redis,
    http: reqwest::Client,
    registry: Arc<OAuthProviderRegistry>,
}

impl OAuthRefreshLoop {
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                if let Err(e) = self.tick().await {
                    tracing::warn!(error = %e, "oauth refresh loop tick failed");
                }
            }
        })
    }

    async fn tick(&self) -> Result<(), RelayError> {
        // 1. 查所有 credential_type='oauth' 且 status=Enabled 的 account
        // 2. 逐个加载 credentials
        // 3. 构造对应 Provider
        // 4. 提前 5 分钟过期的调 get_token 触发 refresh
        // 5. refresh 失败：status = Expired，记录 error_message
        // 6. 每个 account 处理完后 sleep 少量 jitter，避免同时打爆上游 token endpoint
    }
}
```

### 6.3 持久化回调

每个 Provider 构造时注入 `on_refreshed: Arc<dyn Fn(Creds) -> BoxFuture>`，refresh 成功时回调：

```rust
let account_id = account.id;
let db = db.clone();
let redis = redis.clone();
let on_refreshed = Arc::new(move |creds: OAuthCredentials| {
    let db = db.clone();
    let redis = redis.clone();
    async move {
        // UPDATE ai.channel_account SET credentials = jsonb_set(credentials, '{oauth}', $1)
        //   WHERE id = $account_id
        update_oauth_creds(&db, account_id, &creds).await?;
        // 失效 Redis 缓存的 ai:account:{id}
        invalidate_account_cache(&redis, account_id).await?;
        Ok(())
    }.boxed()
});
```

---

## 七、错误映射

```rust
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("authorization_pending")]
    AuthorizationPending,

    #[error("slow_down")]
    SlowDown,

    #[error("expired_token (device_code)")]
    ExpiredToken,

    #[error("access_denied")]
    AccessDenied,

    #[error("invalid_grant (refresh_token failed)")]
    InvalidGrant,

    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("upstream {status}: {body}")]
    Upstream { status: u16, body: String },

    #[error("configuration: {0}")]
    Configuration(&'static str),
}
```

### 7.1 关键行为

| OAuthError 变体 | account status 转移 | 是否告警 |
|---|---|---|
| `AuthorizationPending` / `SlowDown` | 不变（前端继续轮询） | 否 |
| `ExpiredToken` | 不变（前端需重来 device flow） | 否 |
| `AccessDenied` | 不变（用户拒绝） | 否 |
| `InvalidGrant`（refresh 失败） | `Expired` | **是** |
| `Upstream 5xx` | 不变（下次重试） | 仅 log |
| `Network` | 不变（下次重试） | 仅 log |

### 7.2 错误映射到 RelayError

```rust
impl From<OAuthError> for RelayError {
    fn from(e: OAuthError) -> Self {
        match e {
            OAuthError::InvalidGrant => RelayError::Unauthenticated("oauth refresh failed"),
            OAuthError::AccessDenied => RelayError::Unauthenticated("oauth access denied"),
            OAuthError::Network(e) => RelayError::Http(e),
            OAuthError::Upstream { status, body } => RelayError::UpstreamStatus { status, body: body.into() },
            other => RelayError::StreamProcessing(other.to_string()),
        }
    }
}
```

---

## 八、REST API（新增到 admin 模块）

| 路由 | 方法 | Body / Query | 用途 |
|---|---|---|---|
| `/api/admin/ai/oauth/providers` | GET | — | 列出支持的 provider_type 及其配置（scopes / 需不需要 client_id） |
| `/api/admin/ai/oauth/device/start` | POST | `{provider_type, channel_id, account_name}` | 开启 device flow，返 `{session_id, user_code, verification_uri, interval, expires_in}` |
| `/api/admin/ai/oauth/device/poll` | POST | `{session_id}` | 轮询 token，返 `{status: "pending" \| "ok", account_id?, error?}` |
| `/api/admin/ai/oauth/pkce/authorize` | POST | `{provider_type, channel_id, account_name}` | 返 `{session_id, authorize_url}`（带 code_challenge / state） |
| `/api/admin/ai/oauth/pkce/exchange` | POST | `{session_id, code}` | 用 code + code_verifier 换 token |
| `/api/admin/ai/oauth/import` | POST | `{channel_id, account_name, provider_type, credentials_json}` | 粘贴 JSON 导入 |
| `/api/admin/ai/oauth/refresh/{account_id}` | POST | — | 强制触发 refresh，返新 `expires_at` |
| `/api/admin/ai/oauth/revoke/{account_id}` | POST | — | 清空 credentials，account 转 Disabled |

### 8.1 Session 存储

PKCE / Device Flow 的中间态用 Redis：

```
KEY: ai:oauth:session:{session_id}
TTL: expires_in
VALUE: { provider_type, channel_id, account_name, device_code, code_verifier, state, created_at }
```

Redis 不可用时 fallback 内存 HashMap + Mutex（重启丢失，用户重新授权即可）。

---

## 九、运行时集成

### 9.1 `ChannelStore::pick` 扩展

当前（P5 key-picker 方案后）：

```rust
pub async fn pick(&self, model: &str)
  -> Result<Option<(channel::Model, channel_account::Model, String /*raw_token*/)>, RelayError>
```

引入 OAuth 后：

```rust
pub async fn pick(&self, model: &str)
  -> Result<Option<(channel::Model, channel_account::Model, String)>, RelayError>
{
    let (channel, account) = self.weighted_pick(model).await?;
    let token = match account.credential_type.as_str() {
        "api_key" => self.key_picker.pick(&account)?,
        "oauth"   => self.oauth_resolver.resolve(&account).await?,
        other     => return Err(RelayError::MissingConfig("unknown credential_type")),
    };
    Ok(Some((channel, account, token)))
}
```

`oauth_resolver` 是一个薄层，按 `oauth_provider_type` 从 `OAuthProviderRegistry` 找到 provider 实例（每个 account 一个 provider，常驻内存），调 `get_token`。

### 9.2 `OAuthProviderRegistry`

```rust
pub struct OAuthProviderRegistry {
    providers: DashMap<i64 /*account_id*/, Arc<dyn OAuthProvider>>,
}

impl OAuthProviderRegistry {
    pub async fn get_or_build(
        &self,
        account: &channel_account::Model,
        http: &reqwest::Client,
    ) -> Result<Arc<dyn OAuthProvider>, OAuthError> {
        if let Some(p) = self.providers.get(&account.id) {
            return Ok(p.clone());
        }
        let provider = build_provider(account, &self.on_refreshed(account.id))?;
        self.providers.insert(account.id, provider.clone());
        Ok(provider)
    }

    /// DB / Redis 里 credentials 被更新后调用，让下次 get_or_build 重建 provider。
    pub fn invalidate(&self, account_id: i64) {
        self.providers.remove(&account_id);
    }
}
```

---

## 十、前端集成要点

### 10.1 Channel Account 表单

原来：
- credential_type: `api_key`
- credentials 区：多行 textarea（每行一个 key）

新增：
- credential_type: `api_key` / `oauth`（单选）
- 选 `oauth` 后展开：
  - `oauth_provider_type` 下拉（从 `/oauth/providers` 接口拉）
  - 按 provider_type 显示不同 UI：
    - Device Flow: 按钮"开始授权" → 弹框显示 user_code + 二维码 + 倒计时
    - PKCE: 按钮"打开授权页面" → 新标签页 → 回来粘贴 code
    - Pasted: 大 textarea 粘贴 JSON + "解析预览"

### 10.2 OAuth Account 列表

每行显示：
- account name
- provider_type
- expires_at（倒计时；< 24h 变黄，< 1h 变红）
- 状态（Enabled / Expired）
- 操作：手动 refresh / 查看 scopes / 吊销

---

## 十一、分阶段落地计划

### P-OAuth-1 — Pasted + Refresh Loop（2~3 天）

最小可用：管理员从 CLI 工具拿到现成 credentials JSON，粘贴导入，relay 自动 refresh。能接住 80% 的 "Claude Code / Codex CLI" 场景。

- `oauth/` 模块骨架（credentials + error + trait + PastedCredsProvider + refresh_loop）
- `channel_account.oauth_provider_type` 字段 + migration
- `ChannelStore::pick` 分派 + `OAuthProviderRegistry`
- Admin REST: `POST /oauth/import` + `POST /oauth/refresh/{id}` + `POST /oauth/revoke/{id}`
- 前端: 粘贴表单 + 列表 + 倒计时 + 手动刷新按钮

### P-OAuth-2 — Device Flow（2 天）

- `DeviceFlowProvider`
- Admin REST: `POST /oauth/device/start` + `POST /oauth/device/poll`
- 前端: Device Flow UI（弹框 + user_code + 轮询）
- 支持 `github_device` / `codex_device` / `google_device`

### P-OAuth-3 — PKCE Flow（2 天）

- `PkceFlowProvider`
- Admin REST: `POST /oauth/pkce/authorize` + `POST /oauth/pkce/exchange`
- 前端: PKCE UI（粘贴 code 模式，UI-B）
- 支持 `claude_pkce`

### P-OAuth-4 — Copilot Exchange（1 天）

- `CopilotExchanger` 实现
- `DeviceFlowProvider` 注入 exchanger
- 测试 GitHub Copilot 场景

### P-OAuth-5 — 健壮性 / 运维（2 天）

- `Status::Expired` 自动转移 + 告警 hook
- refresh loop 观测指标（refresh_total / refresh_failed_total / expires_at_histogram）
- 单测覆盖：credentials 过期判定、singleflight 去重、refresh 失败 status 转移、refresh loop 完整跑

---

## 十二、风险 / 权衡

| 风险 | 影响 | 缓解 |
|---|---|---|
| 明文存 refresh_token | 泄漏后可生成无限新 token | 本轮接受；生产加 KMS 包装层 |
| refresh_token 泄漏后被上游吊销 | 所有绑该 account 的请求 401 | `Status::Expired` 自动转移 + 告警 |
| 多实例同时 refresh 导致 rate limit | 上游 429 | 本轮单实例；未来加 Redis 分布式锁 |
| 第三方 `client_id` 吊销 | 所有 device flow 401 | 配置放常量，应急时热更改代码 + 重启 |
| PKCE redirect_uri 指向我们域名 | 需注册 OAuth app | 本轮用 UI-B（粘贴 code），不依赖 redirect_uri |
| Token exchange 缓存错过失效 | 请求 401 | 每次 get_token 前检查 exchange.expires_at |

---

## 十三、测试策略

### 单元测试

- `OAuthCredentials::is_expired`（过去 / 未来 / 缺失 `expires_at` / 边界 3 分钟）
- `OAuthCredentials::parse_credentials_json`（合法 / 缺 access_token / 非 JSON）
- `ExchangeStrategy` form 和 JSON 两种 body 序列化
- `DeviceFlowProvider::poll` 解析 authorization_pending / slow_down / expired_token / access_denied / success
- Singleflight 并发 refresh 只打一次上游（用 mock http）

### 集成测试

- 用 `wiremock` 起假 GitHub token endpoint
- 完整跑一次 device flow 到 `get_token` 成功
- Refresh loop 触发 refresh → 验证 DB 里 credentials 更新 + Redis cache 失效
- refresh 失败 → 验证 account.status 转 `Expired`

### E2E（手动）

- 真实 GitHub Copilot：`/oauth/device/start` → 手动授权 → 能代理一次 /v1/chat/completions
- 真实 Claude Code：粘贴 `~/.claude/credentials.json` 内容 → 能代理一次 /v1/messages
- 真实 Codex CLI：同上

---

## 十四、附：关键代码 skeleton

### `core/src/oauth/credentials.rs`

```rust
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::oauth::error::OAuthError;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OAuthCredentials {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<ExchangeCache>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExchangeCache {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl OAuthCredentials {
    /// 提前 3 分钟视为过期。
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(exp) => now + Duration::minutes(3) >= exp,
            None => true,
        }
    }

    pub fn parse_credentials_json(raw: &str) -> Result<Self, OAuthError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(OAuthError::Configuration("empty credentials json"));
        }
        let mut creds: Self = serde_json::from_str(trimmed)?;
        if creds.access_token.is_empty() {
            return Err(OAuthError::Configuration("access_token is empty"));
        }
        if !creds.refresh_token.is_empty() && creds.expires_at.is_none() {
            creds.expires_at = Some(Utc::now() + Duration::hours(1));
        }
        Ok(creds)
    }
}
```

### `core/src/oauth/provider.rs`

```rust
use async_trait::async_trait;
use reqwest::Client;

use crate::oauth::{OAuthCredentials, OAuthError};

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    async fn get_token(&self, http: &Client) -> Result<String, OAuthError>;
    fn current_credentials(&self) -> OAuthCredentials;
    fn provider_type(&self) -> &'static str;
}
```

### `core/src/oauth/pasted.rs`

```rust
pub struct PastedCredsProvider {
    creds: tokio::sync::RwLock<OAuthCredentials>,
    strategy: Box<dyn ExchangeStrategy>,
    token_url: String,
    on_refreshed: OnRefreshedCallback,
    refresh_inflight: tokio::sync::Mutex<()>,  // 简易 singleflight
}

#[async_trait]
impl OAuthProvider for PastedCredsProvider {
    async fn get_token(&self, http: &Client) -> Result<String, OAuthError> {
        {
            let guard = self.creds.read().await;
            if !guard.is_expired(Utc::now()) {
                return Ok(guard.access_token.clone());
            }
        }
        let _lock = self.refresh_inflight.lock().await;
        // double-check after lock
        {
            let guard = self.creds.read().await;
            if !guard.is_expired(Utc::now()) {
                return Ok(guard.access_token.clone());
            }
        }
        let current = self.creds.read().await.clone();
        let fresh = self.strategy.refresh(http, &self.token_url, &current).await?;
        *self.creds.write().await = fresh.clone();
        (self.on_refreshed)(fresh.clone()).await;
        Ok(fresh.access_token)
    }

    fn current_credentials(&self) -> OAuthCredentials {
        self.creds.blocking_read().clone()
    }

    fn provider_type(&self) -> &'static str { "pasted" }
}
```

---

## 十五、迭代建议

本文档完成后，**建议按 P-OAuth-1 先落地 Pasted 模式**，验证整条链路（entity / credentials / provider / refresh_loop / handler 集成 / 前端表单）无误再铺开 Device Flow / PKCE。Pasted 模式的工作量最小，能覆盖相当多实际使用场景（开发者自己有 CLI 凭证），风险最低。

Device Flow / PKCE 的价值在于"非技术用户也能接入"，但需要前端 UX 投入（弹窗 + 倒计时 + 状态机），建议放第二批。
