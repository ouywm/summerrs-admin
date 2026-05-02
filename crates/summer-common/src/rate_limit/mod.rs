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
//! - **无锁热路径**：GCRA 与 ScheduledSlot 使用 [`std::sync::atomic::AtomicI64`] CAS 实现。
//!
//! ## 决策与可观测性
//!
//! [`RateLimitDecision`] 区分四种结果，每种都带 [`RateLimitMetadata`]
//! （limit / remaining / reset_after / retry_after），可用于：
//!
//! - 设置 HTTP `RateLimit-*` / `Retry-After` 头（见 [`middleware`] 模块）
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
//!
//! ## Redis 版本要求
//!
//! - **最低 Redis 6.0**：`Reservation::commit/release` 走的 GCRA refund 脚本依赖
//!   `SET ... KEEPTTL` 选项（6.0 引入）。如果你的 Redis < 6.0 请避免使用 reservation API。
//! - **Redis Cluster 兼容**：所有 user key 段会被自动包裹 hash tag（`{user-key}`），
//!   保证 Lua 脚本里动态拼接的派生 key（如 fixed_window 的 `:{window_id}` 后缀）
//!   落在同一 hash slot，避免 `CROSSSLOT` 错误。
//!
//! ## 模块组织
//!
//! - [`config`]：配置类型（算法 / per / key 类型 / failure policy / mode / RateLimitConfig）
//! - [`decision`]：决策、metadata、统计计数器
//! - [`context`]：HTTP handler 用的 [`RateLimitContext`] + axum extractor
//! - [`reservation`]：[`Reservation`] RAII 凭证
//! - [`engine`]：[`RateLimitEngine`] 主类型
//! - [`algorithms`]：各算法的 memory + redis 实现（在 engine 上做 inherent impl 拓展）
//! - [`middleware`]：HTTP `RateLimit-*` 响应头注入

pub mod algorithms;
pub mod config;
pub mod context;
pub mod decision;
pub mod engine;
pub mod middleware;
pub mod reservation;

pub use config::{
    RateLimitAlgorithm, RateLimitBackend, RateLimitConfig, RateLimitFailurePolicy,
    RateLimitKeyType, RateLimitMode, RateLimitPer,
};
pub use context::RateLimitContext;
pub use decision::{
    RateLimitDecision, RateLimitMetadata, RateLimitMetadataHolder, RateLimitStats,
    RateLimitStatsSnapshot,
};
pub use engine::{
    DEFAULT_MEMORY_CAPACITY, DEFAULT_MEMORY_IDLE_SECS, RateLimitEngine, RateLimitEngineConfig,
};
pub use reservation::Reservation;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use summer_web::axum::http::HeaderMap;

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

    // ---- Cost-Based ----

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

    // ---- Allowlist / Blocklist ----

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

    /// blocklist 命中的 metadata 不带 `retry_after`（避免攻击者立即重试）。
    /// 同时 allowed/rejected counter 不应该被双重计数。
    #[tokio::test]
    async fn blocklist_no_double_count_no_retry_after() {
        let engine = RateLimitEngine::with_config(
            None,
            RateLimitEngineConfig {
                blocklist: vec!["1.2.3.0/24".parse().unwrap()],
                ..Default::default()
            },
        );
        let c = ctx_with_engine("1.2.3.4".parse().unwrap(), None, engine.clone());
        let cfg = config(RateLimitAlgorithm::TokenBucket, 100, 100);

        for _ in 0..5 {
            let _ = c.check("k", cfg.clone(), "blocked").await;
        }
        let stats = engine.stats().snapshot();
        assert_eq!(stats.blocklist_blocks, 5);
        // 关键：rejected counter 不应该被加（之前的 bug 会让它也 +5）
        assert_eq!(stats.rejected, 0);
        assert_eq!(stats.allowed, 0);
    }

    /// allowlist 命中也不应该污染 allowed counter。
    #[tokio::test]
    async fn allowlist_no_double_count_to_allowed() {
        let engine = RateLimitEngine::with_config(
            None,
            RateLimitEngineConfig {
                allowlist: vec!["10.0.0.0/8".parse().unwrap()],
                ..Default::default()
            },
        );
        let c = ctx_with_engine("10.0.0.5".parse().unwrap(), None, engine.clone());
        let cfg = config(RateLimitAlgorithm::TokenBucket, 1, 1);

        for _ in 0..3 {
            c.check("k", cfg.clone(), "limited").await.unwrap();
        }
        let stats = engine.stats().snapshot();
        assert_eq!(stats.allowlist_passes, 3);
        // 关键：allowed counter 不应该被加（之前的 bug 会让它也 +3）
        assert_eq!(stats.allowed, 0);
    }

    // ---- Shadow Mode ----

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

    // ---- reset_key ----

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

    // ---- fail_open / fail_closed / fallback ----

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

    /// 给一个超小容量的内存 cache 灌入 1000 个唯一 key，验证 moka 真的执行 eviction
    /// 且不 panic、不脏读 —— 之前的并发测试只跑 200 个 key 远低于默认 100k 容量，
    /// 实际**没触发**驱逐。
    #[tokio::test]
    async fn moka_evicts_when_capacity_exceeded() {
        let engine = RateLimitEngine::with_config(
            None,
            RateLimitEngineConfig {
                memory_capacity: 50,
                memory_idle: Duration::from_secs(60),
                ..Default::default()
            },
        );
        let cfg = config(RateLimitAlgorithm::TokenBucket, 5, 5);

        for i in 0..1000 {
            let _ = engine.check(&format!("ip:k:{i}"), &cfg).await;
        }

        // moka 是 async eviction：写完后驱逐任务可能还在队列里，强制 flush。
        engine.gcra_states.run_pending_tasks();
        let count = engine.gcra_states.entry_count();
        // moka W-TinyLFU 在 capacity 边界附近会有少量超额（瞬时窗口），用 2x 上限验证。
        assert!(count <= 100, "expected ≤ 100 entries, got {count}");
    }

    /// 验证 Prometheus exporter helper 数组长度 + 名称符合规范。
    #[tokio::test]
    async fn stats_as_metrics_returns_12_named_counters() {
        let engine = RateLimitEngine::new(None);
        let cfg = config(RateLimitAlgorithm::TokenBucket, 5, 5);
        let _ = engine.check("k", &cfg).await;
        let snap = engine.stats().snapshot();
        let metrics = snap.as_metrics();
        assert_eq!(metrics.len(), 12);
        for (name, _) in &metrics {
            assert!(name.starts_with("rate_limit_"));
            assert!(name.ends_with("_total"));
        }
    }

    /// 验证 sanitize_user_key 在超长输入下走 hash 路径。
    #[tokio::test]
    async fn long_user_keys_are_hashed() {
        use super::algorithms::sanitize_user_key;
        let short = "user:42";
        assert_eq!(sanitize_user_key(short), short);

        let long = "x".repeat(500);
        let hashed = sanitize_user_key(&long);
        assert!(hashed.starts_with("h:"));
        assert_eq!(hashed.len(), 34); // "h:" + 32 hex chars
    }

    // ---- Redis 端集成（仅本地有 redis 时启用）----

    async fn redis_or_skip() -> Option<summer_redis::Redis> {
        // `Client::open` 只解析 URL，不真的建连；`get_connection_manager` 创建延迟
        // 连接对象但首次命令才会真正握手。如果 Redis 端口被防火墙挡 / 密码不对 /
        // 服务挂了，这两步可能都"成功"但 PING 时才失败。所以用 PING 兜底防假阳性。
        let client = summer_redis::redis::Client::open("redis://127.0.0.1/").ok()?;
        let mut conn = client.get_connection_manager().await.ok()?;
        let pong: Option<String> = summer_redis::redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .ok();
        if pong.as_deref() == Some("PONG") {
            Some(conn)
        } else {
            None
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
