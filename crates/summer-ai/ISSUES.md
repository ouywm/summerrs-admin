# summer-ai 问题清单

更新时间：2026-03-31

本文档基于对 `summer-ai` 全部三个子 crate 的逐文件源码审计，按严重程度 × 模块分类整理。

---

## 目录

- [概览统计](#概览统计)
- [一、🔴 Critical — 必须立即修复](#一-critical--必须立即修复)
- [二、🟠 High — 近期修复](#二-high--近期修复)
- [三、🟡 Medium — 计划修复](#三-medium--计划修复)
- [四、🔵 Low — 优化建议](#四-low--优化建议)
- [五、跨层系统性问题](#五跨层系统性问题)
- [六、修复优先级排序](#六修复优先级排序)

---

## 概览统计

| 严重程度 | 数量 | 说明 |
|---|---|---|
| 🔴 Critical | 6 | 可导致数据损坏、资金损失、服务不可用 |
| 🟠 High | 10 | 影响正确性或安全性，但有一定容错 |
| 🟡 Medium | 12 | 影响健壮性或可维护性 |
| 🔵 Low | 8 | 性能优化或代码质量改善 |
| **总计** | **36** | |

---

## 一、🔴 Critical — 必须立即修复

### C-01：SSE 流解析 UTF-8 字符分裂损坏

**位置**：`core/src/provider/openai.rs:53`, `anthropic.rs:274`, `gemini.rs:298`

**问题**：三个 provider 的 `parse_stream` 都使用 `String::from_utf8_lossy` 将网络字节块追加到字符串缓冲区。当多字节 UTF-8 字符（中文、emoji 等）被拆分到两个 TCP 包时，`lossy` 会用 `�` (U+FFFD) 替换，造成**不可逆的内容损坏**。

```rust
// 三个 adapter 都有这个模式
buffer.push_str(&String::from_utf8_lossy(&chunk));
```

**影响**：在高延迟/小包场景下，中文/日文/emoji 内容会随机损坏。用户看到乱码。

**修复方案**：

方案 A：字节级缓冲（推荐）
```rust
let mut byte_buffer = Vec::new();
byte_buffer.extend_from_slice(&chunk);
// 在 byte_buffer 中搜索 b"\n\n" 分隔符
// 只有完整事件才转为 String
while let Some(pos) = find_double_newline(&byte_buffer) {
    let event_bytes = byte_buffer.drain(..pos + 2).collect::<Vec<_>>();
    let event_text = String::from_utf8(event_bytes)
        .map_err(|e| anyhow::anyhow!("invalid UTF-8 in SSE event: {e}"))?;
    // ... 解析 event_text
}
```

方案 B：`utf-8` crate 有状态解码器
```rust
use utf8::BufReadDecoder;
// 或手动：尾部不完整的 UTF-8 序列保留到下一次读取
```

---

### C-02：RouteHealthService 非原子 Read-Modify-Write

> 状态（2026-03-31）：已修复。`route_health.rs` 已改为 Redis Hash 原子计数更新，不再使用 `GET -> 内存修改 -> SET`。

**位置**：`hub/src/service/route_health.rs:106-131`

**问题**：`mutate_snapshot` 执行 `GET → 内存修改 → SET` 三步操作，不是原子的。在高并发下，两个请求同时失败，各自读到 `penalty_count=0`，各自加 1 写回，结果只记录了 1 次惩罚而不是 2 次。

```rust
async fn mutate_snapshot<F>(&self, key: &str, mutate: F) -> ApiResult<bool> {
    let mut snapshot = self.cache.get_json(key).await?.unwrap_or_default();  // GET
    let original = snapshot.clone();
    mutate(&mut snapshot);                                                    // MODIFY
    self.cache.set_json(key, &snapshot, TTL).await?;                          // SET (不是 CAS)
}
```

**影响**：路由健康评分不准确 → 选路偏差 → 不健康渠道得到过多流量，或健康渠道被过度惩罚。

**修复方案**：使用 Redis Lua 脚本实现原子操作：
```lua
-- KEYS[1] = snapshot key, ARGV[1] = field_to_increment, ARGV[2] = ttl
local data = redis.call('GET', KEYS[1])
local snapshot = data and cjson.decode(data) or {penalty=0, rate_limit=0, overload=0}
snapshot[ARGV[1]] = snapshot[ARGV[1]] + 1
redis.call('SETEX', KEYS[1], ARGV[2], cjson.encode(snapshot))
return snapshot[ARGV[1]]
```

或者将三个计数器改为独立的 Redis `HINCRBY` 字段。

---

### C-03：BillingEngine DB 扣款与 Redis 记录不原子

**位置**：`hub/src/relay/billing.rs:96-141`

**问题**：`pre_consume` 先执行数据库 `UPDATE` 扣减配额（L97-107），成功后才写 Redis 记录（L121-124）。如果进程在两步之间崩溃：
- 数据库已扣款
- Redis 没有记录
- 后续既无法 settle 也无法 refund → **用户额度永久丢失**

```rust
// 第一步：DB 扣款（已持久化）
let result = token::Entity::update_many()
    .col_expr(token::Column::RemainQuota, Expr::col(...).sub(quota))
    .exec(&self.db).await?;

// 第二步：Redis 写记录（可能失败/崩溃前执行不到）
let inserted = self.cache.set_json_if_absent(&record_key, &record, TTL).await;
```

**影响**：进程崩溃时用户配额被静默扣减，无法恢复。

**修复方案**：

方案 A：先写 Redis 记录（状态 = Pending），再扣 DB，最后更新 Redis（状态 = Reserved）
```
Redis SET (Pending) → DB UPDATE → Redis SET (Reserved)
```
如果 DB 失败，清理 Redis Pending 记录即可。如果在 DB 成功后 Redis 更新失败，下次请求会看到 Pending 记录并补偿。

方案 B：数据库事务 + outbox 表，定时补偿。

---

### C-04：流式结算 Fire-and-Forget 丢失风险

> 状态（2026-03-31）：主流式链路已修复。`relay::stream.rs` 与通用资源流转发链路已改为在流结束的同一 future 内完成结算/退款/限流收尾；非流式 detached accounting 仍可继续收敛。

**位置**：`hub/src/relay/stream.rs:93-146`

**问题**：流式请求的计费结算在 `tokio::spawn` 中异步执行。如果服务器收到 SIGTERM 在流结束之后但 spawn 任务执行之前：
- `post_consume` 永远不会执行
- 用户不付费
- 限流计数器不归零（并发计数泄漏）

```rust
tokio::spawn(async move {
    // post_consume + log + rate_limit finalize
    // 全部在后台执行，无等待机制
});
```

**影响**：用户获得免费调用 + 并发限流计数器永久 +1。

**修复方案**：
1. 使用 `tokio::task::JoinSet` 或 `GracefulShutdown` 追踪后台任务
2. 收到 shutdown 信号时等待所有结算任务完成（带超时）
3. 或使用 outbox + 补偿机制确保最终一致

---

### C-05：Multipart 文件上传无大小限制 (OOM)

**位置**：`hub/src/router/openai.rs` (`buffer_multipart_fields`), `audio_transcribe.rs`, `image_multipart.rs`

**问题**：`field.bytes().await` 将整个上传文件**完整读入内存**，没有大小限制。攻击者发送 10GB 文件 → 服务器 OOM panic。

```rust
// 将整个文件字节全部加载到内存
let data = field.bytes().await?;
```

**影响**：单个恶意请求可使服务器 OOM 崩溃，DoS 攻击向量。

**修复方案**：
```rust
const MAX_FILE_SIZE: usize = 512 * 1024 * 1024; // 512MB
let mut buf = Vec::new();
while let Some(chunk) = field.chunk().await? {
    buf.extend_from_slice(&chunk);
    if buf.len() > MAX_FILE_SIZE {
        return Err(ApiErrors::PayloadTooLarge("file exceeds 512MB limit"));
    }
}
```
或直接使用 Axum 的 `DefaultBodyLimit` 中间件。

---

### C-06：RateLimit incr + expire 非原子 (Key 永不过期)

**位置**：`hub/src/relay/rate_limit.rs:68-73`, `85-90`

**问题**：`INCR` 和 `EXPIRE` 是两个独立 Redis 命令。如果进程在 `INCR` 之后、`EXPIRE` 之前崩溃，该 key 没有 TTL → 永不过期 → 该 token 的 RPM/TPM 配额被**永久占用**。

```rust
let current = self.cache.incr(&key).await?;     // INCR（key 无 TTL）
if current == 1 {
    self.cache.expire(&key, TTL).await?;          // EXPIRE（可能执行不到）
}
```

**影响**：特定 token 永久无法发送新请求（限流 key 永不释放）。

**修复方案**：
```rust
// 方案 A：使用 Lua 脚本合并为原子操作
local current = redis.call('INCR', KEYS[1])
if current == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return current

// 方案 B：每次 INCR 后都设置 EXPIRE（幂等安全）
let current = self.cache.incr(&key).await?;
self.cache.expire(&key, TTL).await?;  // 每次都设置，消除窗口
```

---

## 二、🟠 High — 近期修复

### H-01：Token 配额缓存 60 秒窗口

> 状态（2026-03-31）：已缓解。验证缓存 TTL 已从 60 秒收紧到 10 秒；更彻底的方案仍是把 `remain_quota` 从缓存载荷中移除。

**位置**：`hub/src/service/token.rs` (validate 方法)

**问题**：`TokenInfo`（含 `remain_quota`）被 Redis 缓存 60 秒。用户在这 60 秒窗口内可以绕过配额检查。虽然 `pre_consume` 的 DB `WHERE remain_quota >= ?` 是最终防线，但在高并发场景下，数十个请求可能同时通过缓存检查，全部到达 DB 层，造成大量 DB 失败请求。

**修复方案**：
- 将 `remain_quota` 从 `TokenInfo` 缓存中移除，改为实时查询
- 或者将缓存 TTL 缩短到 5-10 秒

---

### H-02：ChannelAccount credentials 敏感信息泄露

> 状态（2026-03-31）：已修复。`ChannelAccountVo.credentials` 与 `ChannelDetailVo.config` 已从 API 序列化结果与 schema 中移除。

**位置**：`model/src/vo/channel_account.rs:14`, `model/src/vo/channel.rs:81`

**问题**：`ChannelAccountVo` 和 `ChannelDetailVo` 直接暴露 `credentials` 和 `config` 字段到 API 响应。这些字段包含上游 API Key、Secret Key 等敏感信息。

**修复方案**：
- `credentials` 字段在 VO 中只返回脱敏版本（如 `sk-...xxxx`）
- 或用 `#[serde(skip_serializing)]` 完全不输出
- 新增单独的 `GET /api/ai/channel-account/{id}/credentials` 接口，需要额外权限

---

### H-03：ChannelService extract_api_key panic 风险

> 状态（2026-03-31）：已修复。当前实现已不再对 credentials JSON 使用 `expect`，该问题保留为历史记录。

**位置**：`hub/src/service/channel.rs:1096`

**问题**：`extract_api_key` 在解析 `credentials` JSON 字符串时使用 `.expect("extract api key")`。如果数据库中存在格式不正确的 credentials 字段（历史数据、手动修改），会直接 **panic** 导致整个请求线程崩溃。

```rust
let parsed: Value = serde_json::from_str(&credentials).expect("extract api key");
```

**修复方案**：
```rust
let parsed: Value = serde_json::from_str(&credentials)
    .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("invalid credentials JSON: {e}")))?;
```

---

### H-04：ChannelRouter 无 SingleFlight 机制

**位置**：`hub/src/relay/channel_router.rs:309-372`

**问题**：`load_cached_route_candidates` 在 Redis 缓存未命中时，会执行重量级的 DB 多表 JOIN + Account 加载。在高并发场景下，缓存同时过期会触发 **Cache Stampede**：N 个并发请求同时查库。

**修复方案**：
```rust
// 使用 tokio::sync::OnceCell 或 moka 的 entry API
let result = self.in_flight_cache
    .try_get_with(cache_key, async {
        load_from_db().await
    })
    .await;
```

---

### H-05：ChannelRouter N+1 Redis 查询

**位置**：`hub/src/relay/channel_router.rs:620-629`

**问题**：在候选渠道循环中，每个 account 独立调用 `load_account_snapshot` 和 `load_channel_snapshot`，产生 2N 次 Redis GET。如果有 50 个候选 account，就是 100 次 Redis 往返。

**修复方案**：使用 Redis MGET 批量获取：
```rust
let keys: Vec<String> = accounts.iter()
    .flat_map(|a| vec![account_key(a.id), channel_key(a.channel_id)])
    .collect();
let results = self.cache.mget(&keys).await?;
```

---

### H-06：每次请求 spawn DB 写入 LastUsedIp

**位置**：`hub/src/router/openai/completions.rs:70`

**问题**：每个 AI 请求都 `tokio::spawn` 一个 DB UPDATE 写入 `last_used_ip`。高并发下对数据库连接池造成大量压力，且这个数据的时效性不敏感。

**修复方案**：
- 批量写入：像 `LogBatchQueue` 一样攒批后写
- 或改为定期快照：在 Redis 记录 last_ip，每 5 分钟同步到 DB

---

### H-07：流式结算判定过于严格

**位置**：`hub/src/relay/stream.rs:214-224`

**问题**：`resolve_stream_settlement` 在有 `usage` 但没有 `finish_reason` 时判定为 Failure 并全额退款。部分 provider（特别是 Ollama、某些 OpenAI 兼容代理）不一定返回 `finish_reason`，导致用户已收到完整内容但系统认为失败并退款。

```rust
(Some(_), false) => StreamSettlement::Failure {
    status_code: 0,
    message: "stream ended before terminal finish_reason".into(),
},
```

**修复方案**：
- 有 `usage` + 有 `completion_tokens > 0` → 视为成功（即使无 finish_reason）
- 或改为部分结算（按实际 usage 结算，不全额退款）

---

### H-08：Anthropic/Gemini 工具调用 arguments JSON 类型不安全

**位置**：`core/src/provider/anthropic.rs:788`, `gemini.rs:1189`

**问题**：function arguments 解析失败时 fallback 为 `Value::String(arguments)`。如果上游返回一个合法 JSON 但不是 Object（如 `"hello"` 或 `[1,2,3]`），下游期望 Object 的代码会失败。

```rust
serde_json::from_str(arguments).unwrap_or_else(|_| serde_json::Value::String(arguments.into()))
```

**修复方案**：
```rust
match serde_json::from_str::<Value>(arguments) {
    Ok(v) if v.is_object() => v,
    Ok(v) => serde_json::json!({ "raw": v }),  // 包装非 Object
    Err(_) => serde_json::json!({ "raw": arguments }),
}
```

---

### H-09：BigDecimal ↔ f64 转换精度丢失

**位置**：`model/src/dto/channel_account.rs:63`, `model/src/vo/model_config.rs:34`, `hub/src/relay/billing.rs:340-342`

**问题**：多处 `BigDecimal` 转 `f64` 通过 `f64::from_str(&bd.to_string())`，反向通过 `f64::to_string()` 创建 `BigDecimal`。这在边界值时会丢失精度或产生科学计数法。

```rust
// VO 层
f64::from_str(&bd.to_string()).unwrap_or(1.0)
// DTO 层
BigDecimal::from_str(&self.rate_multiplier.to_string())
```

**修复方案**：使用 `BigDecimal` 的 `ToPrimitive` trait：
```rust
use num_traits::ToPrimitive;
bd.to_f64().unwrap_or(1.0)
```

---

### H-10：ChannelType enum 无法处理未知 Provider 类型

**位置**：`model/src/entity/_entity/channel.rs:53`

**问题**：`ChannelType` 是严格枚举。如果数据库中存储了代码尚未定义的 provider 编号（如新迁移或手动写入），整个 Channel 列表查询会因反序列化失败而报错。

**修复方案**：
```rust
#[sea_orm(num_value)]
pub enum ChannelType {
    OpenAi = 1,
    Anthropic = 3,
    // ...
    #[sea_orm(fallback)]
    Unknown = -1,  // 或用 Other(i32) pattern
}
```

---

## 三、🟡 Medium — 计划修复

### M-01：SSE 事件分隔符只检查 `\n\n`

**位置**：`core/src/provider/openai.rs:56`, `anthropic.rs:276`, `gemini.rs:300`

**问题**：SSE 规范允许 `\n\n`、`\r\n\r\n`、`\r\r` 作为事件分隔符。当前只检查 `\n\n`。某些代理或 CDN 可能重写换行符。

**修复方案**：
```rust
fn find_event_boundary(buf: &str) -> Option<(usize, usize)> {
    for (pattern, len) in [("\r\n\r\n", 4), ("\n\n", 2), ("\r\r", 2)] {
        if let Some(pos) = buf.find(pattern) {
            return Some((pos, len));
        }
    }
    None
}
```

---

### M-02：Token 估算精度极低

**位置**：`hub/src/relay/billing.rs:475-484`, `core/src/types/` 多处

**问题**：所有 token 估算都用 `len / 4.0` 硬编码。对中文（约 1-2 字 = 1 token）误差可达 300%；对 JSON 内容（含大量语法字符）也会高估。

```rust
(total_chars as f64 / 4.0).ceil() as i32
```

**影响**：预扣配额偏差大 → 退款频繁 → 限流估算不准。

**修复方案**：
- 短期：区分语言 —— 中文 `len / 2.0`，英文 `len / 4.0`
- 中期：引入 `tiktoken-rs` 做精确估算（主流模型）
- 兜底：对 `serde_json::Value` 提取纯文本而非序列化后计算

---

### M-03：BillingEngine refund 错误被吞

**位置**：`hub/src/relay/billing.rs:128`, `133`

**问题**：`pre_consume` 中 `set_json_if_absent` 失败时调用 `apply_refund_amount`，但用 `let _` 忽略退款结果。如果退款也失败（DB 异常），用户的钱就丢了。

```rust
Err(error) => {
    let _ = self.apply_refund_amount(token_info.token_id, quota).await;  // 结果被忽略
    return Err(error);
}
```

**修复方案**：
```rust
if let Err(refund_err) = self.apply_refund_amount(token_info.token_id, quota).await {
    tracing::error!(
        "CRITICAL: failed to refund after cache error: refund_err={refund_err}, original_err={error}"
    );
    // 写入死信表 / 告警
}
```

---

### M-04：parse_error 中错误体静默降级

**位置**：`core/src/provider/mod.rs:246`

**问题**：上游错误响应体解析失败时 fallback 为 `json!({})`，完全丢失了错误信息。

```rust
serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}))
```

**修复方案**：至少保留原始响应文本：
```rust
serde_json::from_slice(body).unwrap_or_else(|_| {
    let raw = String::from_utf8_lossy(body);
    serde_json::json!({ "raw_error": raw.to_string() })
})
```

---

### M-05：Tools 解析失败静默忽略

**位置**：`core/src/provider/mod.rs:140`

**问题**：`serde_json::from_value::<Vec<Tool>>(tools.clone()).ok()` 静默忽略无效的 tool 定义。客户端发送了格式错误的 tool，不会收到任何提示。

**修复方案**：解析失败时返回 400 错误。

---

### M-06：UpstreamHttpClient 缺少全局超时

**位置**：`hub/src/relay/http_client.rs:13-19`

**问题**：`reqwest::Client` 只设置了 `connect_timeout` 但没有 `timeout`。如果上游建立连接后无响应（半开连接），请求会无限期挂起，消耗 tokio 任务和文件描述符。

**修复方案**：
```rust
Client::builder()
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(300))  // 添加全局超时
    .build()
```

---

### M-07：core/types/error.rs 依赖 axum

**位置**：`core/src/types/error.rs:293`

**问题**：`core` crate 直接导入 `axum` 类型来构建 HTTP Response。这导致 `core` 无法用于非 Web 场景（CLI 工具、测试工具等），违反了分层解耦原则。

**修复方案**：
- `core` 只定义错误数据结构（`ProviderErrorInfo`）
- HTTP Response 构建移到 `hub` 层

---

### M-08：RuntimeService 全量加载不可扩展

**位置**：`hub/src/service/runtime.rs` (health, routes 方法)

**问题**：`health()` 和 `routes()` 方法从 DB 加载**所有** channel、account、ability 记录到内存。当渠道数增长到上千个时，每次 runtime 查询都是全表扫描。

**修复方案**：
- 分页查询 + 按需加载
- 增加 `channel_type` / `status` 过滤参数
- 对不变数据使用内存缓存

---

### M-09：SeaORM Entity 缺少 Relation 定义

**位置**：`model/src/entity/_entity/*.rs` 全部

**问题**：所有 Entity 都定义了 `Relation = ()` 空枚举，没有声明实际的表间关系。导致无法使用 SeaORM 的 `find_with_related()`、JOIN 查询等功能，Service 层被迫手动执行 N+1 查询。

**修复方案**：
```rust
#[derive(EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::channel_account::Entity")]
    ChannelAccounts,
}

impl Related<super::channel_account::Entity> for Entity {
    fn to() -> RelationDef { Relation::ChannelAccounts.def() }
}
```

---

### M-10：update_time / create_time 时区不明

**位置**：`model/src/entity/channel.rs`, `channel_account.rs`, `token.rs` 等 `before_save`

**问题**：使用 `chrono::Utc::now()` 生成时间戳但数据库列定义为 `TimestampWithTimeZone`，存取过程中时区处理不明确。如果 PostgreSQL 的 `timezone` 设置与 UTC 不一致，可能出现时间偏移。

**修复方案**：确保 SeaORM 配置和 DB 列定义统一为 UTC，或在连接字符串中强制 `SET timezone = 'UTC'`。

---

### M-11：DTO 更新允许修改不安全字段

**位置**：`model/src/dto/channel.rs:105`

**问题**：`UpdateChannelDto` 允许修改 `channel_type` 和 `channel_group`。修改正在使用的 channel 的 type 可能导致 Ability 路由表不一致、Adapter 选择错误、计费数据混乱。

**修复方案**：
- 从 `UpdateChannelDto` 中移除 `channel_type`
- 或在 service 层检查：如果 channel 有关联的 ability/account，禁止修改 type

---

### M-12：LogVo 缺少 cost_total 字段

**位置**：`model/src/vo/log.rs`

**问题**：`LogVo` 没有包含 `cost_total` 字段，尽管 Log 实体有这个字段。前端无法直接展示每次请求的总成本。

---

## 四、🔵 Low — 优化建议

### L-01：SSE 循环字符串分配

**位置**：`core/src/provider/openai.rs:57-58`

**问题**：SSE 解析循环中每次分割事件都创建新的 String：
```rust
let event_text = buffer[..pos].to_string();   // 分配
buffer = buffer[pos + 2..].to_string();       // 再分配
```

**优化**：使用 `String::drain()` 或基于字节偏移的解析。

---

### L-02：Response Builder unwrap

**位置**：`core/src/types/error.rs:293`, `hub/src/router/openai/support.rs`

**问题**：`Response::builder()...body(body.into()).unwrap()` 在极端情况下（无效头等）可能 panic。

**优化**：改为 `.map_err(ApiErrors::Internal)?`。

---

### L-03：渠道恢复 Job 无防抖

**位置**：`hub/src/job/channel_recovery.rs`

**问题**：每 5 分钟探测 AutoDisabled 渠道。如果渠道在"恢复 → 立即失败 → 再恢复"之间反复跳转（flapping），会产生大量无效 DB 写入和路由缓存失效。

**优化**：引入 backoff 策略 — 渠道被恢复后如果再次在短时间内被 disable，下次恢复等待时间翻倍。

---

### L-04：auth 中间件 fail-open 风险

**位置**：`hub/src/auth/middleware.rs:92`

**问题**：`requires_ai_auth` 使用 `starts_with("/v1/") || starts_with("/api/v1/")` 匹配。如果新增不以此前缀开头的敏感接口，会默认不鉴权（fail-open）。

**优化**：改为 fail-close — 默认需要鉴权，显式标记排除列表。

---

### L-05：Soft Delete 不一致

**位置**：Entity 层

**问题**：`Channel` 和 `ChannelAccount` 支持 soft delete（`deleted_at`），但 `Token`、`UserQuota` 不支持。Token 物理删除后无法审计历史使用记录。

---

### L-06：JSON 结构字段缺少 Schema 校验

**位置**：`model/src/dto/channel.rs` (models, capabilities, config 字段)

**问题**：DTO 中大量 `serde_json::Value` 字段没有结构校验。无效 JSON 会被直接持久化到数据库。

**优化**：为关键 JSON 字段定义结构体类型，或在 service 层添加 schema 校验。

---

### L-07：last_error_message 类型不匹配

**位置**：`model/src/entity/_entity/channel_account.rs:95`

**问题**：`last_error_message` 定义为 `String`（非 nullable）。逻辑上"无错误"应该用 `Option<String>` 表示，而不是空字符串。

---

### L-08：ModelService 4 次独立查询

**位置**：`hub/src/service/model.rs:33`

**问题**：`list_available` 执行 4 次独立 DB 查询（abilities, channels, accounts, configs）后在内存中关联过滤。应该使用 JOIN 查询减少 DB 往返。

---

## 五、跨层系统性问题

### S-01：error handling 一致性

当前错误处理存在三种不同模式，缺乏一致性：

| 模式 | 出现位置 | 问题 |
|---|---|---|
| `.ok()` / `unwrap_or` | core 层 parse | 静默丢失错误上下文 |
| `.expect("...")` | service/router 层 | 生产环境 panic |
| `let _ = ...` | billing/rate_limit 层 | 关键操作结果被忽略 |

**建议**：制定统一规范：
- core 层：返回 `Result`，由 hub 层决定如何处理
- hub 层：日志 + 降级，绝不 panic，关键操作结果必须处理
- 所有 `let _ =` 用于关键路径时，必须伴随 `tracing::error!`

### S-02：Redis 降级策略缺失

当前代码中 Redis 失败直接向上传播为错误。如果 Redis 短暂不可用，所有 AI 请求都会失败，即使 PostgreSQL 完全正常。

**建议**：
- 缓存层（route cache、token cache、model config cache）：Redis 失败时 fallback 到 DB
- 限流层：Redis 失败时放行（log warning），不阻塞请求
- 计费层：Redis 失败时 fallback 到纯 DB 模式

### S-03：测试覆盖空白

| 模块 | 现有测试 | 缺失 |
|---|---|---|
| billing.rs | ✅ 端点校验 | ❌ 预扣/结算/退款流程 |
| rate_limit.rs | ❌ | ❌ 全部（仅有 struct 定义） |
| channel_router.rs | ❌ | ❌ 路由选择逻辑 |
| stream.rs | ✅ settlement 判定 | ❌ SSE 解析 |
| route_health.rs | ✅ 基本谓词 | ❌ 并发安全 |
| provider/*.rs | ❌ | ❌ 协议转换正确性 |

**建议**：优先补充 billing 和 rate_limit 的单元测试，这两个模块直接影响用户资金安全。

---

## 六、修复优先级排序

### Sprint 1（本周）— 数据安全

| 编号 | 问题 | 风险 |
|---|---|---|
| C-01 | SSE UTF-8 分裂 | 内容损坏 |
| C-03 | Billing DB/Redis 非原子 | 配额丢失 |
| C-05 | Multipart OOM | DoS |
| C-06 | RateLimit key 永不过期 | 服务不可用 |
| H-03 | extract_api_key panic | 服务崩溃 |

### Sprint 2（下周）— 计费正确性

| 编号 | 问题 | 风险 |
|---|---|---|
| C-02 | RouteHealth 非原子 | 选路偏差 |
| C-04 | 流式结算 fire-and-forget | 免费调用 |
| H-07 | 流式判定过严 | 过度退款 |
| M-03 | refund 错误被吞 | 资金丢失 |

### Sprint 3（两周内）— 安全与健壮性

| 编号 | 问题 | 风险 |
|---|---|---|
| H-01 | Token 缓存 60s 窗口 | 配额绕过 |
| H-02 | Credentials 泄露 | 安全 |
| H-04 | Cache Stampede | 性能 |
| M-06 | HTTP Client 无超时 | 资源泄漏 |

### Sprint 4（一个月内）— 质量提升

| 编号 | 问题 |
|---|---|
| H-05 | ChannelRouter N+1 |
| H-06 | LastUsedIp DB 压力 |
| M-02 | Token 估算精度 |
| M-09 | Entity 缺 Relation |
| S-03 | 测试覆盖 |

### 持续改善

| 编号 | 问题 |
|---|---|
| S-02 | Redis 降级策略 |
| S-01 | 错误处理一致性 |
| M-07 | core 解耦 axum |
| L-03 | 渠道恢复防抖 |
