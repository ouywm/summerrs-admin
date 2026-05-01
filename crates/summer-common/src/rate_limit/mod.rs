//! 顶级限流引擎 —— 对标 Cloudflare / Stripe / Envoy / governor / Helicone
//!
//! ## 算法
//!
//! 内核算法是 [GCRA](https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm)
//! （Generic Cell Rate Algorithm），业界限流标准：
//!
//! - **状态最小**：单变量 TAT (Theoretical Arrival Time) 即可完整描述桶状态。
//! - **整数算术**：纯整数毫秒运算，无浮点漂移。
//! - **原生 burst**：通过 burst tolerance 控制突发容量，无需独立计数器。
//! - **数学等价于 leaky bucket**：与 token bucket 相比更精确（不丢精度）。
//! - **Cost-Based 友好**：通过 cost 参数自然扩展为按消耗计费的限流（LLM TPM、文件大小、金额）。
//!
//! 用户暴露的算法选项：
//!
//! | 选项             | 内核              | 备注                                    |
//! | ---------------- | ----------------- | --------------------------------------- |
//! | `token_bucket`   | GCRA              | burst 默认 = rate（保持 API 兼容）       |
//! | `gcra`           | GCRA              | 显式 GCRA，自定 burst                   |
//! | `leaky_bucket`   | ScheduledSlot     | 等价 GCRA + burst=1 + max_wait=0        |
//! | `throttle_queue` | ScheduledSlot     | leaky bucket 的排队变体（max_wait > 0） |
//! | `fixed_window`   | FixedWindow       | 简单计数器，按自然窗口对齐              |
//! | `sliding_window` | SlidingWindowLog  | ZSET / VecDeque 存请求时间戳            |
//!
//! ## Token Cost-Based 限流（AI 场景）
//!
//! 对 GCRA / TokenBucket 算法，可以通过 [`RateLimitContext::check_with_cost`]
//! 把单次请求消耗的 cost 传入（默认 1）。GCRA 的 emission_interval 自然按 cost
//! 倍数推进 TAT，等价于"该请求消耗 cost 个单位"，对应 LLM 的 TPM / 文件大小 /
//! daily_cost 等场景。
//!
//! 配额预扣 + 退还 / 提交：
//! ```ignore
//! let res = ctx.reserve("user:42", &cfg, 4096).await?;  // 预扣 4096 token
//! match call_llm().await {
//!     Ok(resp) => res.commit(resp.usage.total_tokens).await,  // 实扣
//!     Err(_)   => res.release().await,                         // 全退
//! }
//! ```
//!
//! ## 资源管理
//!
//! 内存状态全部存于 [`moka::sync::Cache`]，带：
//!
//! - **TTL** (`time_to_idle`)：闲置自动回收。
//! - **容量上限** (`max_capacity`)：恶意 IP 注入时仍有界。
//! - **无锁热路径**：GCRA 与 ScheduledSlot 使用 [`AtomicI64`] CAS 实现。
//!
//! ## 决策与可观测性
//!
//! [`RateLimitDecision`] 区分四种结果，每种都带 [`RateLimitMetadata`]
//! （limit / remaining / reset_after / retry_after），可用于：
//!
//! - 设置 HTTP `RateLimit-*` / `Retry-After` 头（见 [`headers_layer`]）
//! - 监控 / 告警（见 [`RateLimitEngine::stats`]）
//!
//! ## 名单（Allowlist / Blocklist）
//!
//! [`RateLimitEngineConfig::allowlist`] 和 `blocklist` 接受 CIDR 网段。
//! - allowlist 命中：直接放行，**不消耗任何配额**
//! - blocklist 命中：直接拒绝（429），不进入算法
//!
//! ## Shadow Mode
//!
//! 设置 `mode = Shadow` 后，命中限流时**只记日志、不真的拒绝**。
//! 用于灰度上线评估规则合理性。
//!
//! ## Backend Failure Policy
//!
//! Redis 不可达时按 [`RateLimitFailurePolicy`] 处理：
//!
//! - `FailOpen`：放行（不再无谓地跑 fallback），仅记 stats。
//! - `FailClosed`：返回 503。
//! - `FallbackMemory`：跌到内存桶（多实例下语义降级）。

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ipnetwork::IpNetwork;
use moka::sync::Cache;
use parking_lot::Mutex;
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;

use crate::error::{ApiErrors, ApiResult};

pub mod middleware;

const REDIS_GCRA_SCRIPT: &str = include_str!("lua/rate_limit_gcra.lua");
const REDIS_GCRA_REFUND_SCRIPT: &str = include_str!("lua/rate_limit_gcra_refund.lua");
const REDIS_FIXED_WINDOW_SCRIPT: &str = include_str!("lua/rate_limit_fixed_window.lua");
const REDIS_SLIDING_WINDOW_SCRIPT: &str = include_str!("lua/rate_limit_sliding_window.lua");
const REDIS_SCHEDULED_SLOT_SCRIPT: &str = include_str!("lua/rate_limit_scheduled_slot.lua");

/// 内存 cache 默认容量（防恶意 IP 注入爆内存）
pub const DEFAULT_MEMORY_CAPACITY: u64 = 100_000;
/// 内存 cache 默认空闲过期时间
pub const DEFAULT_MEMORY_IDLE_SECS: u64 = 3600;

// =============================================================================
// 1. 配置类型
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitPer {
    Second,
    Minute,
    Hour,
    Day,
}

impl RateLimitPer {
    pub fn window_seconds(self) -> u64 {
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::Hour => 3600,
            Self::Day => 86400,
        }
    }

    pub fn window_millis(self) -> i64 {
        (self.window_seconds() * 1000) as i64
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitKeyType {
    Global,
    Ip,
    User,
    Header(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitBackend {
    Memory,
    Redis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitAlgorithm {
    /// 兼容选项；内核同 GCRA，burst 默认 = rate。
    TokenBucket,
    /// 显式 GCRA。
    Gcra,
    /// 内核 ScheduledSlot(max_wait=0)。
    LeakyBucket,
    /// 内核 ScheduledSlot(max_wait>0)。
    ThrottleQueue,
    /// 计数器按自然窗口边界滚动。
    FixedWindow,
    /// 时间戳日志。
    SlidingWindow,
}

impl RateLimitAlgorithm {
    pub fn as_key_segment(self) -> &'static str {
        match self {
            Self::TokenBucket => "token_bucket",
            Self::Gcra => "gcra",
            Self::LeakyBucket => "leaky_bucket",
            Self::ThrottleQueue => "throttle_queue",
            Self::FixedWindow => "fixed_window",
            Self::SlidingWindow => "sliding_window",
        }
    }

    fn uses_gcra(self) -> bool {
        matches!(self, Self::TokenBucket | Self::Gcra)
    }

    fn uses_scheduled_slot(self) -> bool {
        matches!(self, Self::LeakyBucket | Self::ThrottleQueue)
    }

    /// 该算法是否支持 cost-based 限流。
    pub fn supports_cost(self) -> bool {
        self.uses_gcra()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitFailurePolicy {
    /// Redis 失败时直接放行（仅记录 stats / log）。
    FailOpen,
    /// Redis 失败时返回 503。
    FailClosed,
    /// Redis 失败时跌到内存桶（多实例下语义降级，仅当前进程隔离）。
    FallbackMemory,
}

impl RateLimitFailurePolicy {
    fn as_key_segment(self) -> &'static str {
        match self {
            Self::FailOpen => "fail_open",
            Self::FailClosed => "fail_closed",
            Self::FallbackMemory => "fallback_memory",
        }
    }
}

/// 限流执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateLimitMode {
    /// 命中即拒绝（标准模式）
    #[default]
    Enforce,
    /// 命中只记日志，不真的拒绝（灰度评估、上线安全网）
    Shadow,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub rate: u32,
    pub per: RateLimitPer,
    pub burst: u32,
    pub backend: RateLimitBackend,
    pub algorithm: RateLimitAlgorithm,
    pub failure_policy: RateLimitFailurePolicy,
    pub max_wait_ms: u64,
    pub mode: RateLimitMode,
}

impl RateLimitConfig {
    /// GCRA / ScheduledSlot 的"每单位理论间隔"，整数毫秒，向上取整防 0。
    pub fn emission_interval_millis(&self) -> i64 {
        let window_ms = self.per.window_millis().max(1);
        let rate = self.rate.max(1) as i64;
        (window_ms + rate - 1) / rate
    }

    pub fn window_seconds(&self) -> u64 {
        self.per.window_seconds()
    }

    pub fn window_millis(&self) -> i64 {
        self.per.window_millis()
    }

    pub fn window_limit(&self) -> u32 {
        self.rate.max(1)
    }

    /// LeakyBucket 强制 burst=1；其它走 GCRA 的算法用配置 burst（默认 = rate）。
    pub fn effective_burst(&self) -> u32 {
        match self.algorithm {
            RateLimitAlgorithm::LeakyBucket => 1,
            _ => self.burst.max(1),
        }
    }

    /// Redis key 的 TTL（秒）。
    pub fn redis_expire_seconds(&self) -> u64 {
        match self.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                let burst = self.effective_burst().max(1) as u64;
                let interval = self.emission_interval_millis().max(1) as u64;
                (burst.saturating_mul(interval).div_ceil(1000)).max(2) * 2
            }
            RateLimitAlgorithm::FixedWindow | RateLimitAlgorithm::SlidingWindow => {
                self.window_seconds().max(1) * 2
            }
            RateLimitAlgorithm::LeakyBucket => {
                let interval = self.emission_interval_millis().max(1) as u64;
                interval.div_ceil(1000).max(1) * 2
            }
            RateLimitAlgorithm::ThrottleQueue => {
                let interval = self.emission_interval_millis().max(1) as u64;
                let max_wait = self.max_wait_ms.max(1);
                (interval.saturating_add(max_wait)).div_ceil(1000).max(1) * 2
            }
        }
    }

    pub fn signature(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.algorithm.as_key_segment(),
            self.rate,
            self.window_seconds(),
            self.effective_burst(),
            self.max_wait_ms,
        )
    }
}

// =============================================================================
// 2. 决策、Metadata、Holder、Stats
// =============================================================================

/// 限流决策附带的元数据，可用于 HTTP `RateLimit-*` / `Retry-After` 头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitMetadata {
    pub limit: u32,
    pub remaining: u32,
    pub reset_after: Duration,
    pub retry_after: Option<Duration>,
}

impl RateLimitMetadata {
    pub fn rejected(limit: u32, retry_after: Duration) -> Self {
        Self {
            limit,
            remaining: 0,
            reset_after: retry_after,
            retry_after: Some(retry_after),
        }
    }

    /// allowlist / 不限速场景的 metadata。
    pub fn unlimited() -> Self {
        Self {
            limit: u32::MAX,
            remaining: u32::MAX,
            reset_after: Duration::ZERO,
            retry_after: None,
        }
    }
}

/// 跨多次 [`RateLimitContext::check`] 的共享 metadata 持有器。
///
/// 由 axum extractor 在 [`Parts::extensions`] 中注入；响应阶段的 layer 取出
/// 写入 HTTP header。多键复合限流时取**最严格**（remaining 最少）的那一个。
#[derive(Debug, Default)]
pub struct RateLimitMetadataHolder {
    inner: Mutex<Option<RateLimitMetadata>>,
}

impl RateLimitMetadataHolder {
    pub fn record(&self, meta: RateLimitMetadata) {
        let mut slot = self.inner.lock();
        match slot.as_ref() {
            Some(existing) if existing.remaining <= meta.remaining => {}
            _ => *slot = Some(meta),
        }
    }

    pub fn snapshot(&self) -> Option<RateLimitMetadata> {
        *self.inner.lock()
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitDecision {
    Allowed(RateLimitMetadata),
    Delayed {
        delay: Duration,
        meta: RateLimitMetadata,
    },
    Rejected(RateLimitMetadata),
    BackendUnavailable,
}

impl RateLimitDecision {
    pub fn metadata(&self) -> Option<&RateLimitMetadata> {
        match self {
            Self::Allowed(meta) | Self::Rejected(meta) => Some(meta),
            Self::Delayed { meta, .. } => Some(meta),
            Self::BackendUnavailable => None,
        }
    }
}

/// 引擎运行时统计（atomic counter，线程安全），可对接 Prometheus / Datadog。
#[derive(Debug, Default)]
pub struct RateLimitStats {
    pub allowed: AtomicU64,
    pub delayed: AtomicU64,
    pub rejected: AtomicU64,
    pub backend_failures: AtomicU64,
    pub fallback_to_memory: AtomicU64,
    pub fail_open_passes: AtomicU64,
    pub fail_closed_blocks: AtomicU64,
    pub allowlist_passes: AtomicU64,
    pub blocklist_blocks: AtomicU64,
    pub shadow_passes: AtomicU64,
    pub cost_consumed: AtomicU64,
    pub cost_refunded: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct RateLimitStatsSnapshot {
    pub allowed: u64,
    pub delayed: u64,
    pub rejected: u64,
    pub backend_failures: u64,
    pub fallback_to_memory: u64,
    pub fail_open_passes: u64,
    pub fail_closed_blocks: u64,
    pub allowlist_passes: u64,
    pub blocklist_blocks: u64,
    pub shadow_passes: u64,
    pub cost_consumed: u64,
    pub cost_refunded: u64,
}

impl RateLimitStats {
    pub fn snapshot(&self) -> RateLimitStatsSnapshot {
        RateLimitStatsSnapshot {
            allowed: self.allowed.load(Ordering::Relaxed),
            delayed: self.delayed.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            backend_failures: self.backend_failures.load(Ordering::Relaxed),
            fallback_to_memory: self.fallback_to_memory.load(Ordering::Relaxed),
            fail_open_passes: self.fail_open_passes.load(Ordering::Relaxed),
            fail_closed_blocks: self.fail_closed_blocks.load(Ordering::Relaxed),
            allowlist_passes: self.allowlist_passes.load(Ordering::Relaxed),
            blocklist_blocks: self.blocklist_blocks.load(Ordering::Relaxed),
            shadow_passes: self.shadow_passes.load(Ordering::Relaxed),
            cost_consumed: self.cost_consumed.load(Ordering::Relaxed),
            cost_refunded: self.cost_refunded.load(Ordering::Relaxed),
        }
    }

    fn record(&self, decision: &RateLimitDecision) {
        match decision {
            RateLimitDecision::Allowed(_) => {
                self.allowed.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::Delayed { .. } => {
                self.delayed.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::Rejected(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::BackendUnavailable => {
                self.fail_closed_blocks.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

// =============================================================================
// 4. Engine 配置
// =============================================================================

/// [`RateLimitEngine`] 构造参数。
#[derive(Debug, Clone)]
pub struct RateLimitEngineConfig {
    /// 内存 cache 单表最大条目数（防恶意 IP 注入）。默认 100_000。
    pub memory_capacity: u64,
    /// 内存 cache 闲置回收时间。默认 1 小时。
    pub memory_idle: Duration,
    /// 白名单：CIDR 命中直接放行，不消耗任何配额。
    pub allowlist: Vec<IpNetwork>,
    /// 黑名单：CIDR 命中直接拒绝（429）。
    pub blocklist: Vec<IpNetwork>,
}

impl Default for RateLimitEngineConfig {
    fn default() -> Self {
        Self {
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
            memory_idle: Duration::from_secs(DEFAULT_MEMORY_IDLE_SECS),
            allowlist: Vec::new(),
            blocklist: Vec::new(),
        }
    }
}

// =============================================================================
// 5. 上下文（axum extractor）
// =============================================================================

/// 限流上下文，由 axum 的 [`FromRequestParts`] 自动提取。
///
/// 在 [`from_request_parts`] 中注入一个共享的 [`RateLimitMetadataHolder`]
/// 到 [`Parts::extensions`]，[`headers_layer`] 在响应阶段从中读 metadata 写 HTTP 头。
#[derive(Clone)]
pub struct RateLimitContext {
    pub client_ip: IpAddr,
    pub user_id: Option<i64>,
    pub headers: HeaderMap,
    pub engine: RateLimitEngine,
    pub metadata: Arc<RateLimitMetadataHolder>,
}

impl RateLimitContext {
    pub fn extract_key(&self, key_type: RateLimitKeyType) -> String {
        match key_type {
            RateLimitKeyType::Global => "global".to_string(),
            RateLimitKeyType::Ip => format!("ip:{}", self.client_ip),
            RateLimitKeyType::User => self
                .user_id
                .map(|user_id| format!("user:{user_id}"))
                .unwrap_or_else(|| format!("ip:{}", self.client_ip)),
            RateLimitKeyType::Header(name) => self
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| format!("header:{name}:{value}"))
                .unwrap_or_else(|| format!("header:{name}:unknown")),
        }
    }

    /// 标准检查（cost = 1）。
    pub async fn check(
        &self,
        key: &str,
        config: RateLimitConfig,
        message: &str,
    ) -> ApiResult<RateLimitMetadata> {
        self.check_with_cost(key, config, 1, message).await
    }

    /// Token Cost-Based 检查；`cost` 是本次请求消耗的单位数。
    ///
    /// 仅 GCRA 内核的算法（TokenBucket / Gcra）真正按 cost 计算；其他算法忽略
    /// cost（按请求数计数），但仍正常工作。
    pub async fn check_with_cost(
        &self,
        key: &str,
        config: RateLimitConfig,
        cost: u32,
        message: &str,
    ) -> ApiResult<RateLimitMetadata> {
        // ---- 名单短路：在算法之前判断，allowlist 不消耗配额，blocklist 直接拒绝。
        if let Some(decision) = self.engine.check_lists(self.client_ip) {
            self.engine.stats.record(&decision);
            return self.finalize(decision, &config, message).await;
        }

        let cost = cost.max(1);
        let decision = self.engine.check_with_cost(key, &config, cost).await;

        // 记 cost 总量
        if matches!(
            decision,
            RateLimitDecision::Allowed(_) | RateLimitDecision::Delayed { .. }
        ) {
            self.engine
                .stats
                .cost_consumed
                .fetch_add(cost as u64, Ordering::Relaxed);
        }

        self.finalize(decision, &config, message).await
    }

    /// 预扣 `estimated_cost` 个单位的配额，返回 [`Reservation`]，业务结束后必须
    /// 调用 [`Reservation::commit`] 或 [`Reservation::release`]。
    ///
    /// 仅支持 GCRA 内核算法（TokenBucket / Gcra）。
    pub async fn reserve(
        &self,
        key: &str,
        config: RateLimitConfig,
        estimated_cost: u32,
        message: &str,
    ) -> ApiResult<Reservation> {
        if !config.algorithm.supports_cost() {
            return Err(ApiErrors::Internal(anyhow::anyhow!(
                "reserve() only supports cost-based algorithms (token_bucket / gcra), \
                 got `{}`",
                config.algorithm.as_key_segment()
            )));
        }
        let cost = estimated_cost.max(1);
        let _meta = self
            .check_with_cost(key, config.clone(), cost, message)
            .await?;
        Ok(Reservation {
            engine: self.engine.clone(),
            state: Some(ReservationState {
                key: key.to_string(),
                config,
                reserved_cost: cost,
            }),
        })
    }

    /// 把 decision 转成 ApiResult，处理 Shadow 模式 / Delayed sleep / metadata holder。
    async fn finalize(
        &self,
        decision: RateLimitDecision,
        config: &RateLimitConfig,
        message: &str,
    ) -> ApiResult<RateLimitMetadata> {
        // 写入 holder 给响应 layer 使用
        if let Some(meta) = decision.metadata().copied() {
            self.metadata.record(meta);
        }

        match decision {
            RateLimitDecision::Allowed(meta) => Ok(meta),
            RateLimitDecision::Delayed { delay, meta } => {
                // tokio sleep 是 cancel-safe 的——client 断开时 axum drop task，
                // sleep 自然停止；server 端已写入的 TAT 状态不回滚（限流领域标准语义）。
                tokio::time::sleep(delay).await;
                Ok(meta)
            }
            RateLimitDecision::Rejected(meta) if config.mode == RateLimitMode::Shadow => {
                self.engine
                    .stats
                    .shadow_passes
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    rate = config.rate,
                    burst = config.effective_burst(),
                    algorithm = config.algorithm.as_key_segment(),
                    retry_after_ms = meta.retry_after.map(|d| d.as_millis()).unwrap_or(0),
                    "rate-limit shadow mode: would have rejected"
                );
                Ok(meta)
            }
            RateLimitDecision::Rejected(_) => Err(ApiErrors::TooManyRequests(message.to_string())),
            RateLimitDecision::BackendUnavailable => Err(ApiErrors::ServiceUnavailable(
                "限流服务暂时不可用，请稍后再试".to_string(),
            )),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RateLimitContext {
    type Rejection = summer_web::error::WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let client_ip = axum_client_ip::ClientIp::from_request_parts(parts, state)
            .await
            .map(|axum_client_ip::ClientIp(ip)| ip)
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

        let user_id = parts
            .extensions
            .get::<summer_auth::UserSession>()
            .map(|session| session.login_id.user_id);

        let headers = parts.headers.clone();
        let engine = if let Some(engine) = parts.extensions.get::<RateLimitEngine>().cloned() {
            engine
        } else {
            parts.get_component::<RateLimitEngine>()?
        };

        // 共享 metadata holder：多次 check 累积到同一个 holder，最后由响应 layer 读取
        let metadata = if let Some(holder) = parts
            .extensions
            .get::<Arc<RateLimitMetadataHolder>>()
            .cloned()
        {
            holder
        } else {
            let holder = Arc::new(RateLimitMetadataHolder::default());
            parts.extensions.insert(holder.clone());
            holder
        };

        Ok(Self {
            client_ip,
            user_id,
            headers,
            engine,
            metadata,
        })
    }
}

impl summer_web::aide::OperationInput for RateLimitContext {}

// =============================================================================
// 6. Reservation（配额预扣 / 退还）
// =============================================================================

/// 配额预扣的 RAII 凭证。
///
/// 必须显式调用 [`Self::commit`] 或 [`Self::release`]，否则 Drop 时会
/// **异步退还全部预扣**（避免泄露配额）并在日志里 warn。
#[must_use = "Reservation must be consumed via commit() or release(); \
              dropping will refund everything (with a warning) on a best-effort basis"]
pub struct Reservation {
    engine: RateLimitEngine,
    state: Option<ReservationState>,
}

#[derive(Debug)]
struct ReservationState {
    key: String,
    config: RateLimitConfig,
    reserved_cost: u32,
}

impl Reservation {
    /// 提交实际消耗。如果 `actual_cost < reserved_cost` 自动退还差额。
    pub async fn commit(mut self, actual_cost: u32) {
        let Some(state) = self.state.take() else {
            return;
        };
        let actual = actual_cost.min(state.reserved_cost);
        if actual < state.reserved_cost {
            let refund = state.reserved_cost - actual;
            self.engine.refund(&state.key, &state.config, refund).await;
        }
    }

    /// 全额退还（业务失败 / 取消时）。
    pub async fn release(mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        self.engine
            .refund(&state.key, &state.config, state.reserved_cost)
            .await;
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            tracing::warn!(
                key = %state.key,
                cost = state.reserved_cost,
                "Reservation dropped without commit/release; auto-refunding"
            );
            // 仅在 tokio runtime 中 spawn；运行时关闭时静默退化
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let engine = self.engine.clone();
                handle.spawn(async move {
                    engine
                        .refund(&state.key, &state.config, state.reserved_cost)
                        .await;
                });
            }
        }
    }
}

// =============================================================================
// 7. Engine
// =============================================================================

/// 内存状态，按算法分桶存储于不同 cache。
struct FixedWindowState {
    window_id: i64,
    count: u32,
}

/// 限流引擎。`Clone` 廉价（内部全 `Arc`）。
#[derive(Clone)]
pub struct RateLimitEngine {
    /// GCRA / TokenBucket / Gcra 共享：value 是 TAT (theoretical arrival time, ms)。
    gcra_states: Cache<String, Arc<AtomicI64>>,
    /// FixedWindow 状态（窗口 id + 计数）。
    fixed_window_states: Cache<String, Arc<Mutex<FixedWindowState>>>,
    /// SlidingWindow 时间戳日志。
    sliding_window_states: Cache<String, Arc<Mutex<VecDeque<i64>>>>,
    /// LeakyBucket / ThrottleQueue 共享：value 是 next_available_ms。
    scheduled_states: Cache<String, Arc<AtomicI64>>,
    redis: Option<summer_redis::Redis>,
    stats: Arc<RateLimitStats>,
    allowlist: Arc<Vec<IpNetwork>>,
    blocklist: Arc<Vec<IpNetwork>>,
}

impl RateLimitEngine {
    pub fn new(redis: Option<summer_redis::Redis>) -> Self {
        Self::with_config(redis, RateLimitEngineConfig::default())
    }

    pub fn with_config(redis: Option<summer_redis::Redis>, cfg: RateLimitEngineConfig) -> Self {
        let build_atomic = |capacity: u64, idle: Duration| -> Cache<String, Arc<AtomicI64>> {
            Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(idle)
                .build()
        };
        let build_fixed =
            |capacity: u64, idle: Duration| -> Cache<String, Arc<Mutex<FixedWindowState>>> {
                Cache::builder()
                    .max_capacity(capacity)
                    .time_to_idle(idle)
                    .build()
            };
        let build_sliding =
            |capacity: u64, idle: Duration| -> Cache<String, Arc<Mutex<VecDeque<i64>>>> {
                Cache::builder()
                    .max_capacity(capacity)
                    .time_to_idle(idle)
                    .build()
            };

        Self {
            gcra_states: build_atomic(cfg.memory_capacity, cfg.memory_idle),
            fixed_window_states: build_fixed(cfg.memory_capacity, cfg.memory_idle),
            sliding_window_states: build_sliding(cfg.memory_capacity, cfg.memory_idle),
            scheduled_states: build_atomic(cfg.memory_capacity, cfg.memory_idle),
            redis,
            stats: Arc::new(RateLimitStats::default()),
            allowlist: Arc::new(cfg.allowlist),
            blocklist: Arc::new(cfg.blocklist),
        }
    }

    /// 暴露给业务层做监控接入。
    pub fn stats(&self) -> &RateLimitStats {
        &self.stats
    }

    /// 名单短路：allowlist → 直接 Allowed；blocklist → 直接 Rejected。
    pub fn check_lists(&self, client_ip: IpAddr) -> Option<RateLimitDecision> {
        if !self.allowlist.is_empty() && self.allowlist.iter().any(|net| net.contains(client_ip)) {
            self.stats.allowlist_passes.fetch_add(1, Ordering::Relaxed);
            return Some(RateLimitDecision::Allowed(RateLimitMetadata::unlimited()));
        }
        if !self.blocklist.is_empty() && self.blocklist.iter().any(|net| net.contains(client_ip)) {
            self.stats.blocklist_blocks.fetch_add(1, Ordering::Relaxed);
            return Some(RateLimitDecision::Rejected(RateLimitMetadata::rejected(
                0,
                Duration::ZERO,
            )));
        }
        None
    }

    pub async fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        self.check_with_cost(key, config, 1).await
    }

    pub async fn check_with_cost(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let cost = cost.max(1);
        let decision = match config.backend {
            RateLimitBackend::Memory => self.check_memory(key, config, cost),
            RateLimitBackend::Redis => self.check_redis(key, config, cost).await,
        };
        self.stats.record(&decision);
        decision
    }

    /// 重置某个 key 的限流状态（运维干预）。
    pub fn reset_key(&self, key: &str, config: &RateLimitConfig) {
        let cache_key = format!("{}:{}", config.signature(), key);
        match config.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                self.gcra_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::FixedWindow => {
                self.fixed_window_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::SlidingWindow => {
                self.sliding_window_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::LeakyBucket | RateLimitAlgorithm::ThrottleQueue => {
                self.scheduled_states.invalidate(&cache_key);
            }
        }
        // Redis 端：尝试删除（best effort）
        if matches!(config.backend, RateLimitBackend::Redis)
            && let Some(redis) = self.redis.clone()
        {
            let redis_key = self.redis_key_for(key, config);
            tokio::spawn(async move {
                let mut conn = redis;
                let _: Result<i64, _> = summer_redis::redis::cmd("DEL")
                    .arg(redis_key)
                    .query_async(&mut conn)
                    .await;
            });
        }
    }

    /// 退还 `cost` 个单位的配额（仅 GCRA 内核生效）。
    pub async fn refund(&self, key: &str, config: &RateLimitConfig, cost: u32) {
        if !config.algorithm.supports_cost() || cost == 0 {
            return;
        }
        let emission = config.emission_interval_millis();
        let refund_ms = (cost as i64).saturating_mul(emission);

        match config.backend {
            RateLimitBackend::Memory => {
                let cache_key = format!("{}:{}", config.signature(), key);
                if let Some(state) = self.gcra_states.get(&cache_key) {
                    // fetch_sub atomic；TAT 可能临时低于 now，下次 check 用 max(tat, now) 兜底
                    state.fetch_sub(refund_ms, Ordering::AcqRel);
                }
            }
            RateLimitBackend::Redis => {
                let Some(redis) = &self.redis else {
                    return;
                };
                let redis_key = self.redis_key_for(key, config);
                let mut conn = redis.clone();
                let now_ms = current_time_millis();
                let result = summer_redis::redis::Script::new(REDIS_GCRA_REFUND_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(emission)
                    .arg(cost as i64)
                    .invoke_async::<i64>(&mut conn)
                    .await;
                if let Err(error) = result {
                    tracing::warn!(error = %error, key, cost, "rate-limit refund failed");
                }
            }
        }
        self.stats
            .cost_refunded
            .fetch_add(cost as u64, Ordering::Relaxed);
    }

    fn redis_key_for(&self, key: &str, config: &RateLimitConfig) -> String {
        format!(
            "rate-limit:{}:{}:{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            config.effective_burst(),
            config.max_wait_ms,
            key,
        )
    }

    // -------- Memory 端 --------

    fn check_memory(&self, key: &str, config: &RateLimitConfig, cost: u32) -> RateLimitDecision {
        if config.algorithm.uses_gcra() {
            self.check_memory_gcra(key, config, cost)
        } else if config.algorithm.uses_scheduled_slot() {
            let max_wait_ms = if config.algorithm == RateLimitAlgorithm::LeakyBucket {
                0
            } else {
                config.max_wait_ms
            };
            self.check_memory_scheduled_slot(key, config, max_wait_ms)
        } else {
            match config.algorithm {
                RateLimitAlgorithm::FixedWindow => self.check_memory_fixed_window(key, config),
                RateLimitAlgorithm::SlidingWindow => self.check_memory_sliding_window(key, config),
                _ => unreachable!("algorithm dispatch must be exhaustive"),
            }
        }
    }

    /// GCRA / TokenBucket 共享路径，CAS 无锁，支持 cost > 1。
    fn check_memory_gcra(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let emission = config.emission_interval_millis();
        let burst = config.effective_burst() as i64;
        let cost = cost.max(1) as i64;
        let capacity = burst * emission;
        let cost_emission = cost * emission;
        let limit = burst as u32;

        let cache_key = format!("{}:{}", config.signature(), key);
        let state = self
            .gcra_states
            .get_with(cache_key, || Arc::new(AtomicI64::new(now_ms)));

        loop {
            let tat = state.load(Ordering::Acquire);
            let arrival = tat.max(now_ms);
            let diff = arrival - now_ms;

            // cost-based GCRA: 推进后桶超容 → 拒绝
            if diff + cost_emission > capacity {
                let retry_after_ms = (diff + cost_emission - capacity).max(0) as u64;
                let retry_after = Duration::from_millis(retry_after_ms);
                return RateLimitDecision::Rejected(RateLimitMetadata::rejected(
                    limit,
                    retry_after,
                ));
            }

            let new_tat = arrival + cost_emission;
            if state
                .compare_exchange(tat, new_tat, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let remaining = if emission > 0 {
                    ((capacity - diff - cost_emission) / emission).max(0) as u32
                } else {
                    0
                };
                let reset_after = Duration::from_millis((new_tat - now_ms).max(0) as u64);
                return RateLimitDecision::Allowed(RateLimitMetadata {
                    limit,
                    remaining,
                    reset_after,
                    retry_after: None,
                });
            }
        }
    }

    fn check_memory_fixed_window(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let window_ms = config.window_millis().max(1);
        let window_id = now_ms.div_euclid(window_ms);
        let limit = config.window_limit();

        let cache_key = format!("{}:{}", config.signature(), key);
        let state = self.fixed_window_states.get_with(cache_key, || {
            Arc::new(Mutex::new(FixedWindowState {
                window_id,
                count: 0,
            }))
        });

        let mut s = state.lock();
        if s.window_id != window_id {
            s.window_id = window_id;
            s.count = 0;
        }

        let window_end_ms = (window_id + 1) * window_ms;
        let reset_after = Duration::from_millis(window_end_ms.saturating_sub(now_ms).max(0) as u64);

        if s.count >= limit {
            return RateLimitDecision::Rejected(RateLimitMetadata {
                limit,
                remaining: 0,
                reset_after,
                retry_after: Some(reset_after),
            });
        }
        s.count += 1;
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining: limit.saturating_sub(s.count),
            reset_after,
            retry_after: None,
        })
    }

    fn check_memory_sliding_window(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let window_ms = config.window_millis().max(1);
        let limit = config.window_limit();

        let cache_key = format!("{}:{}", config.signature(), key);
        let state = self
            .sliding_window_states
            .get_with(cache_key, || Arc::new(Mutex::new(VecDeque::new())));

        let mut entries = state.lock();
        let cutoff = now_ms - window_ms;
        while entries.front().is_some_and(|ts| *ts <= cutoff) {
            entries.pop_front();
        }

        if entries.len() as u32 >= limit {
            let retry_after_ms = entries
                .front()
                .map(|oldest| (oldest + window_ms - now_ms).max(0) as u64)
                .unwrap_or(0);
            let retry_after = Duration::from_millis(retry_after_ms);
            return RateLimitDecision::Rejected(RateLimitMetadata {
                limit,
                remaining: 0,
                reset_after: Duration::from_millis(window_ms as u64),
                retry_after: Some(retry_after),
            });
        }

        entries.push_back(now_ms);
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining: limit.saturating_sub(entries.len() as u32),
            reset_after: Duration::from_millis(window_ms as u64),
            retry_after: None,
        })
    }

    fn check_memory_scheduled_slot(
        &self,
        key: &str,
        config: &RateLimitConfig,
        max_wait_ms: u64,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let interval_ms = config.emission_interval_millis().max(1);
        let limit = 1u32;

        let cache_key = format!("{}:{}", config.signature(), key);
        let state = self
            .scheduled_states
            .get_with(cache_key, || Arc::new(AtomicI64::new(now_ms)));

        loop {
            let next_available = state.load(Ordering::Acquire);
            let scheduled = next_available.max(now_ms);
            let delay_ms = scheduled.saturating_sub(now_ms).max(0) as u64;

            if delay_ms > max_wait_ms {
                let retry_after = Duration::from_millis(delay_ms);
                return RateLimitDecision::Rejected(RateLimitMetadata::rejected(
                    limit,
                    retry_after,
                ));
            }

            let new_next = scheduled + interval_ms;
            if state
                .compare_exchange(
                    next_available,
                    new_next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let meta = RateLimitMetadata {
                    limit,
                    remaining: 0,
                    reset_after: Duration::from_millis(interval_ms as u64),
                    retry_after: None,
                };
                if delay_ms == 0 {
                    return RateLimitDecision::Allowed(meta);
                }
                return RateLimitDecision::Delayed {
                    delay: Duration::from_millis(delay_ms),
                    meta,
                };
            }
        }
    }

    // -------- Redis 端 --------

    async fn check_redis(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let Some(redis) = &self.redis else {
            return self.handle_backend_failure(key, config, cost);
        };

        let now_ms = current_time_millis();
        let expire_seconds = config.redis_expire_seconds() as i64;
        let redis_key = self.redis_key_for(key, config);
        let mut conn = redis.clone();

        let result = match config.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                let burst = config.effective_burst().max(1) as i64;
                let emission = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_GCRA_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(emission)
                    .arg(burst)
                    .arg(expire_seconds)
                    .arg(cost as i64)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            burst as u32,
                            Duration::from_millis(emission as u64),
                        )
                    })
            }
            RateLimitAlgorithm::FixedWindow => {
                let limit = config.window_limit();
                summer_redis::redis::Script::new(REDIS_FIXED_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(limit as i64)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            limit,
                            Duration::from_millis(config.window_millis() as u64),
                        )
                    })
            }
            RateLimitAlgorithm::SlidingWindow => {
                let limit = config.window_limit();
                summer_redis::redis::Script::new(REDIS_SLIDING_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(limit as i64)
                    .arg(format!("{now_ms}:{}", uuid::Uuid::new_v4()))
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            limit,
                            Duration::from_millis(config.window_millis() as u64),
                        )
                    })
            }
            RateLimitAlgorithm::LeakyBucket => {
                let interval = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(interval)
                    .arg(0_i64)
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, _) = unpack_triple(&values);
                        scheduled_decision_from_lua(
                            allowed,
                            value_ms,
                            Duration::from_millis(interval as u64),
                        )
                    })
            }
            RateLimitAlgorithm::ThrottleQueue => {
                let interval = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(interval)
                    .arg(config.max_wait_ms as i64)
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, _) = unpack_triple(&values);
                        scheduled_decision_from_lua(
                            allowed,
                            value_ms,
                            Duration::from_millis(interval as u64),
                        )
                    })
            }
        };

        match result {
            Ok(decision) => decision,
            Err(error) => {
                self.stats.backend_failures.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    error = %error,
                    key,
                    rate = config.rate,
                    burst = config.burst,
                    algorithm = %config.algorithm.as_key_segment(),
                    failure_policy = %config.failure_policy.as_key_segment(),
                    window_seconds = config.window_seconds(),
                    max_wait_ms = config.max_wait_ms,
                    "redis rate limit check failed; applying failure policy"
                );
                self.handle_backend_failure(key, config, cost)
            }
        }
    }

    fn handle_backend_failure(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        match config.failure_policy {
            RateLimitFailurePolicy::FailOpen => {
                self.stats.fail_open_passes.fetch_add(1, Ordering::Relaxed);
                let limit = config.effective_burst().max(1);
                RateLimitDecision::Allowed(RateLimitMetadata {
                    limit,
                    remaining: limit,
                    reset_after: Duration::ZERO,
                    retry_after: None,
                })
            }
            RateLimitFailurePolicy::FailClosed => RateLimitDecision::BackendUnavailable,
            RateLimitFailurePolicy::FallbackMemory => {
                self.stats
                    .fallback_to_memory
                    .fetch_add(1, Ordering::Relaxed);
                self.check_memory(key, config, cost)
            }
        }
    }
}

// =============================================================================
// 8. Lua 解析辅助
// =============================================================================

fn unpack_triple(values: &[i64]) -> (i64, i64, i64) {
    (
        values.first().copied().unwrap_or(0),
        values.get(1).copied().unwrap_or(0),
        values.get(2).copied().unwrap_or(0),
    )
}

fn decision_from_lua_triple(
    allowed: i64,
    value_ms: i64,
    remaining: i64,
    limit: u32,
    reset_after: Duration,
) -> RateLimitDecision {
    let remaining = remaining.max(0) as u32;
    let value = value_ms.max(0) as u64;
    if allowed == 1 {
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining,
            reset_after,
            retry_after: None,
        })
    } else {
        let retry_after = Duration::from_millis(value);
        RateLimitDecision::Rejected(RateLimitMetadata {
            limit,
            remaining: 0,
            reset_after,
            retry_after: Some(retry_after),
        })
    }
}

fn scheduled_decision_from_lua(
    allowed: i64,
    value_ms: i64,
    reset_after: Duration,
) -> RateLimitDecision {
    let value = value_ms.max(0) as u64;
    let limit = 1u32;
    if allowed == 1 {
        let meta = RateLimitMetadata {
            limit,
            remaining: 0,
            reset_after,
            retry_after: None,
        };
        if value == 0 {
            RateLimitDecision::Allowed(meta)
        } else {
            RateLimitDecision::Delayed {
                delay: Duration::from_millis(value),
                meta,
            }
        }
    } else {
        let retry_after = Duration::from_millis(value);
        RateLimitDecision::Rejected(RateLimitMetadata::rejected(limit, retry_after))
    }
}

fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

// =============================================================================
// 9. Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn config(algorithm: RateLimitAlgorithm, rate: u32, burst: u32) -> RateLimitConfig {
        RateLimitConfig {
            rate,
            per: RateLimitPer::Second,
            burst,
            backend: RateLimitBackend::Memory,
            algorithm,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
            mode: RateLimitMode::Enforce,
        }
    }

    fn ctx(client_ip: IpAddr, user_id: Option<i64>) -> RateLimitContext {
        ctx_with_engine(client_ip, user_id, RateLimitEngine::new(None))
    }

    fn ctx_with_engine(
        client_ip: IpAddr,
        user_id: Option<i64>,
        engine: RateLimitEngine,
    ) -> RateLimitContext {
        RateLimitContext {
            client_ip,
            user_id,
            headers: HeaderMap::new(),
            engine,
            metadata: Arc::new(RateLimitMetadataHolder::default()),
        }
    }

    // ---- 已有测试 ----

    #[tokio::test]
    async fn extract_user_falls_back_to_ip_with_prefix() {
        let c = ctx("127.0.0.1".parse().unwrap(), None);
        assert_eq!(c.extract_key(RateLimitKeyType::User), "ip:127.0.0.1");
    }

    #[tokio::test]
    async fn extract_ip_has_prefix() {
        let c = ctx("10.0.0.1".parse().unwrap(), None);
        assert_eq!(c.extract_key(RateLimitKeyType::Ip), "ip:10.0.0.1");
    }

    #[tokio::test]
    async fn extract_user_uses_user_id_when_present() {
        let c = ctx("127.0.0.1".parse().unwrap(), Some(42));
        assert_eq!(c.extract_key(RateLimitKeyType::User), "user:42");
    }

    #[tokio::test]
    async fn gcra_token_bucket_allows_burst_then_rejects() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 2, 2);
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn gcra_explicit_with_burst_one_acts_like_leaky_bucket() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::Gcra, 1, 1);
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn gcra_remaining_decreases_per_request() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 5, 5);
        let m1 = match engine.check("k", &cfg).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!("expected allowed"),
        };
        let m2 = match engine.check("k", &cfg).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!("expected allowed"),
        };
        assert!(m1.remaining > m2.remaining);
        assert_eq!(m1.limit, 5);
    }

    #[tokio::test]
    async fn fixed_window_aligns_and_resets() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::FixedWindow, 2, 0);
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn sliding_window_provides_retry_after_from_oldest() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::SlidingWindow, 2, 0);
        engine.check("k", &cfg).await;
        engine.check("k", &cfg).await;
        let d = engine.check("k", &cfg).await;
        if let RateLimitDecision::Rejected(meta) = d {
            assert!(meta.retry_after.unwrap() <= Duration::from_secs(1));
        } else {
            panic!("expected rejected");
        }
    }

    #[tokio::test]
    async fn leaky_bucket_rejects_until_interval_passes() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::LeakyBucket, 1, 1);
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
    }

    #[tokio::test]
    async fn throttle_queue_delays_then_rejects_when_overloaded() {
        let engine = RateLimitEngine::new(None);
        let mut cfg = config(RateLimitAlgorithm::ThrottleQueue, 1, 0);
        cfg.max_wait_ms = 1500;

        let d1 = engine.check("k", &cfg).await;
        assert!(matches!(d1, RateLimitDecision::Allowed(_)));

        let d2 = engine.check("k", &cfg).await;
        match d2 {
            RateLimitDecision::Delayed { delay, .. } => {
                assert!(delay >= Duration::from_millis(900));
            }
            _ => panic!("expected delayed: {d2:?}"),
        }

        let d3 = engine.check("k", &cfg).await;
        assert!(matches!(d3, RateLimitDecision::Rejected(_)));
    }

    // ---- 新增：Cost-Based ----

    #[tokio::test]
    async fn cost_based_consumes_burst_proportionally() {
        // capacity = 10, 单次 cost=4，应该允许两次后第三次拒绝
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 10);
        assert!(matches!(
            engine.check_with_cost("k", &cfg, 4).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check_with_cost("k", &cfg, 4).await,
            RateLimitDecision::Allowed(_)
        ));
        // 已经消耗 8 个，剩 2 个；这次再扣 4 个会超
        assert!(matches!(
            engine.check_with_cost("k", &cfg, 4).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn cost_based_remaining_reflects_cost() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 100);
        let m = match engine.check_with_cost("k", &cfg, 30).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!(),
        };
        assert_eq!(m.limit, 100);
        // 消耗 30 后应该剩 70
        assert_eq!(m.remaining, 70);
    }

    #[tokio::test]
    async fn reservation_commit_refunds_difference() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 100);
        let c = ctx_with_engine("127.0.0.1".parse().unwrap(), None, engine.clone());

        // 预扣 50
        let res = c
            .reserve("k", cfg.clone(), 50, "rate limited")
            .await
            .unwrap();
        // 实际消耗 20，应退还 30
        res.commit(20).await;

        // 验证：cost_consumed=50, cost_refunded=30, 净消耗 20
        let stats = engine.stats().snapshot();
        assert_eq!(stats.cost_consumed, 50);
        assert_eq!(stats.cost_refunded, 30);

        // 桶应该剩 100-20=80
        let m = match engine.check_with_cost("k", &cfg, 1).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!(),
        };
        // 剩 80-1=79
        assert_eq!(m.remaining, 79);
    }

    #[tokio::test]
    async fn reservation_release_refunds_all() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 100);
        let c = ctx_with_engine("127.0.0.1".parse().unwrap(), None, engine.clone());

        let res = c.reserve("k", cfg.clone(), 50, "limited").await.unwrap();
        res.release().await;

        let stats = engine.stats().snapshot();
        assert_eq!(stats.cost_consumed, 50);
        assert_eq!(stats.cost_refunded, 50);

        // 桶应该完全恢复
        let m = match engine.check_with_cost("k", &cfg, 1).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!(),
        };
        assert_eq!(m.remaining, 99);
    }

    #[tokio::test]
    async fn reserve_rejects_for_non_cost_algorithm() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::FixedWindow, 100, 0);
        let c = ctx_with_engine("127.0.0.1".parse().unwrap(), None, engine);
        let result = c.reserve("k", cfg, 10, "limited").await;
        assert!(result.is_err());
    }

    // ---- 新增：Allowlist / Blocklist ----

    #[tokio::test]
    async fn allowlist_passes_unconditionally() {
        let engine = RateLimitEngine::with_config(
            None,
            RateLimitEngineConfig {
                allowlist: vec!["10.0.0.0/8".parse().unwrap()],
                ..Default::default()
            },
        );
        let c = ctx_with_engine("10.0.0.5".parse().unwrap(), None, engine.clone());
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);

        // 即使消耗超过 burst，也允许（不消耗配额）
        for _ in 0..100 {
            assert!(c.check("k", cfg.clone(), "limited").await.is_ok());
        }
        let stats = engine.stats().snapshot();
        assert_eq!(stats.allowlist_passes, 100);
    }

    #[tokio::test]
    async fn blocklist_blocks_unconditionally() {
        let engine = RateLimitEngine::with_config(
            None,
            RateLimitEngineConfig {
                blocklist: vec!["1.2.3.0/24".parse().unwrap()],
                ..Default::default()
            },
        );
        let c = ctx_with_engine("1.2.3.4".parse().unwrap(), None, engine.clone());
        let cfg = config(RateLimitAlgorithm::TokenBucket, 100, 100);

        // 黑名单 → 直接拒绝
        let result = c.check("k", cfg, "blocked").await;
        assert!(result.is_err());
        assert_eq!(engine.stats().snapshot().blocklist_blocks, 1);
    }

    // ---- 新增：Shadow Mode ----

    #[tokio::test]
    async fn shadow_mode_records_but_does_not_reject() {
        let engine = RateLimitEngine::new(None);
        let c = ctx_with_engine("127.0.0.1".parse().unwrap(), None, engine.clone());
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);
        cfg.mode = RateLimitMode::Shadow;

        // 第一个允许
        c.check("k", cfg.clone(), "limited").await.unwrap();
        // 第二个本应被拒，但 shadow 模式仍放行
        c.check("k", cfg.clone(), "limited").await.unwrap();
        c.check("k", cfg, "limited").await.unwrap();

        let stats = engine.stats().snapshot();
        assert!(stats.shadow_passes >= 2);
    }

    // ---- 新增：reset_key ----

    #[tokio::test]
    async fn reset_key_clears_bucket() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);

        // 耗尽桶
        engine.check("k", &cfg).await;
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));

        // 重置
        engine.reset_key("k", &cfg);

        // 恢复
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
    }

    // ---- 已有：fail_open / fail_closed / fallback ----

    #[tokio::test]
    async fn fail_open_skips_fallback_and_records_stats() {
        let engine = RateLimitEngine::new(None);
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);
        cfg.backend = RateLimitBackend::Redis;
        cfg.failure_policy = RateLimitFailurePolicy::FailOpen;

        for _ in 0..1000 {
            assert!(matches!(
                engine.check("k", &cfg).await,
                RateLimitDecision::Allowed(_)
            ));
        }
        let stats = engine.stats().snapshot();
        assert_eq!(stats.fail_open_passes, 1000);
        assert_eq!(stats.allowed, 1000);
        assert_eq!(stats.fallback_to_memory, 0);
    }

    #[tokio::test]
    async fn fail_closed_returns_backend_unavailable() {
        let engine = RateLimitEngine::new(None);
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);
        cfg.backend = RateLimitBackend::Redis;
        cfg.failure_policy = RateLimitFailurePolicy::FailClosed;

        let d = engine.check("k", &cfg).await;
        assert!(matches!(d, RateLimitDecision::BackendUnavailable));
        assert_eq!(engine.stats().snapshot().fail_closed_blocks, 1);
    }

    #[tokio::test]
    async fn fallback_memory_uses_memory_state() {
        let engine = RateLimitEngine::new(None);
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);
        cfg.backend = RateLimitBackend::Redis;
        cfg.failure_policy = RateLimitFailurePolicy::FallbackMemory;

        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check("k", &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn moka_eviction_does_not_panic_under_concurrency() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 5, 5);

        let mut handles = vec![];
        for i in 0..200 {
            let e = engine.clone();
            let c = cfg.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let _ = e.check(&format!("ip:10.0.0.{i}"), &c).await;
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let snap = engine.stats().snapshot();
        assert!(snap.allowed + snap.rejected > 0);
    }

    // ---- Redis 端集成（仅本地有 redis 时启用）----

    async fn redis_or_skip() -> Option<summer_redis::Redis> {
        match summer_redis::redis::Client::open("redis://127.0.0.1/") {
            Ok(client) => client.get_connection_manager().await.ok(),
            Err(_) => None,
        }
    }

    #[tokio::test]
    async fn redis_gcra_respects_burst() {
        let Some(redis) = redis_or_skip().await else {
            return;
        };
        let engine = RateLimitEngine::new(Some(redis));
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 3);
        cfg.backend = RateLimitBackend::Redis;
        let key = format!("redis:gcra:{}", uuid::Uuid::new_v4());

        for _ in 0..3 {
            assert!(matches!(
                engine.check(&key, &cfg).await,
                RateLimitDecision::Allowed(_)
            ));
        }
        assert!(matches!(
            engine.check(&key, &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn redis_cost_based_with_refund() {
        let Some(redis) = redis_or_skip().await else {
            return;
        };
        let engine = RateLimitEngine::new(Some(redis));
        let mut cfg = config(RateLimitAlgorithm::TokenBucket, 1, 100);
        cfg.backend = RateLimitBackend::Redis;
        let key = format!("redis:cost:{}", uuid::Uuid::new_v4());

        // 预扣 50
        let m1 = match engine.check_with_cost(&key, &cfg, 50).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!(),
        };
        assert_eq!(m1.remaining, 50);

        // 退还 30
        engine.refund(&key, &cfg, 30).await;

        // 现在应该剩 80（100 - 50 + 30）
        let m2 = match engine.check_with_cost(&key, &cfg, 1).await {
            RateLimitDecision::Allowed(m) => m,
            _ => panic!(),
        };
        // 80 - 1 = 79
        assert_eq!(m2.remaining, 79);
    }

    #[tokio::test]
    async fn redis_fixed_window_aligns_to_natural_window() {
        let Some(redis) = redis_or_skip().await else {
            return;
        };
        let engine = RateLimitEngine::new(Some(redis));
        let mut cfg = config(RateLimitAlgorithm::FixedWindow, 2, 0);
        cfg.backend = RateLimitBackend::Redis;
        let key = format!("redis:fixed:{}", uuid::Uuid::new_v4());

        assert!(matches!(
            engine.check(&key, &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check(&key, &cfg).await,
            RateLimitDecision::Allowed(_)
        ));
        assert!(matches!(
            engine.check(&key, &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }

    #[tokio::test]
    async fn redis_sliding_window_returns_retry_after() {
        let Some(redis) = redis_or_skip().await else {
            return;
        };
        let engine = RateLimitEngine::new(Some(redis));
        let mut cfg = config(RateLimitAlgorithm::SlidingWindow, 2, 0);
        cfg.backend = RateLimitBackend::Redis;
        let key = format!("redis:sliding:{}", uuid::Uuid::new_v4());

        engine.check(&key, &cfg).await;
        engine.check(&key, &cfg).await;
        let d = engine.check(&key, &cfg).await;
        match d {
            RateLimitDecision::Rejected(meta) => {
                assert!(meta.retry_after.unwrap() <= Duration::from_secs(1));
            }
            _ => panic!("expected rejected"),
        }
    }

    #[tokio::test]
    async fn redis_throttle_queue_delays_under_capacity() {
        let Some(redis) = redis_or_skip().await else {
            return;
        };
        let engine = RateLimitEngine::new(Some(redis));
        let mut cfg = config(RateLimitAlgorithm::ThrottleQueue, 1, 0);
        cfg.backend = RateLimitBackend::Redis;
        cfg.max_wait_ms = 1500;
        let key = format!("redis:queue:{}", uuid::Uuid::new_v4());

        let d1 = engine.check(&key, &cfg).await;
        assert!(matches!(d1, RateLimitDecision::Allowed(_)));
        let d2 = engine.check(&key, &cfg).await;
        match d2 {
            RateLimitDecision::Delayed { delay, .. } => {
                assert!(delay >= Duration::from_millis(900));
            }
            _ => panic!("expected delayed: {d2:?}"),
        }
        assert!(matches!(
            engine.check(&key, &cfg).await,
            RateLimitDecision::Rejected(_)
        ));
    }
}
