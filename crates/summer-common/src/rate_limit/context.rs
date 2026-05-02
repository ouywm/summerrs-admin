//! HTTP handler 用的限流上下文 + axum extractor。

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;

use crate::error::{ApiErrors, ApiResult};
use crate::rate_limit::config::{RateLimitConfig, RateLimitKeyType, RateLimitMode};
use crate::rate_limit::decision::{
    RateLimitDecision, RateLimitMetadata, RateLimitMetadataHolder, SharedMetadataHolder,
};
use crate::rate_limit::engine::RateLimitEngine;
use crate::rate_limit::reservation::{Reservation, ReservationState};

/// `RateLimitContext` 提取 client_ip 失败时只 warn 一次，避免每请求刷屏。
pub(crate) static IP_FALLBACK_WARNED: OnceLock<()> = OnceLock::new();
/// 非 GCRA 算法收到 `cost > 1` 时只 warn 一次。
pub(crate) static COST_IGNORED_WARNED: OnceLock<()> = OnceLock::new();

/// 限流上下文，由 axum 的 [`FromRequestParts`] 自动提取。
///
/// 在 [`Self::from_request_parts`] 中注入一个共享的 [`RateLimitMetadataHolder`]
/// 到 [`Parts::extensions`]，[`super::middleware::rate_limit_headers_middleware`]
/// 在响应阶段从中读 metadata 写 HTTP 头。
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
        //
        // 注意：list 命中已在 `check_lists` 内更新了专属计数器（allowlist_passes /
        // blocklist_blocks），这里**不**再走 `stats.record`，否则 allowed / rejected
        // 会被双重计数。同时 list 决策也不走 Shadow 转换 —— blocklist 是永久拉黑，
        // 让它被 Shadow 豁免会让黑名单完全失效。
        if let Some(decision) = self.engine.check_lists(self.client_ip) {
            if let Some(meta) = decision.metadata().copied() {
                self.metadata.record(meta);
            }
            return match decision {
                RateLimitDecision::Allowed(meta) => Ok(meta),
                RateLimitDecision::Rejected(_) => {
                    Err(ApiErrors::TooManyRequests(message.to_string()))
                }
                _ => Err(ApiErrors::Internal(anyhow::anyhow!(
                    "check_lists returned unexpected decision"
                ))),
            };
        }

        // 非 GCRA 内核且 cost > 1 时静默忽略 → 改成首次 warn 提示开发者。
        if cost > 1 && !config.algorithm.supports_cost() {
            COST_IGNORED_WARNED.get_or_init(|| {
                tracing::warn!(
                    algorithm = config.algorithm.as_key_segment(),
                    cost,
                    "rate-limit: cost > 1 was passed for an algorithm that does not \
                     support cost-based metering; counted as 1. Switch to `token_bucket` \
                     or `gcra` if you need cost-based limiting."
                );
            });
        }

        let cost = cost.max(1);
        let decision = self.engine.check_with_cost(key, &config, cost).await;

        // 记 cost 总量（仅算法路径计入 cost_consumed，list 短路不计入）
        if matches!(
            decision,
            RateLimitDecision::Allowed(_) | RateLimitDecision::Delayed { .. }
        ) {
            self.engine
                .stats()
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
            // 构造时捕获 handle，让 Drop 在非 runtime 线程也能退还
            handle: tokio::runtime::Handle::try_current().ok(),
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
                    .stats()
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
        let client_ip = match axum_client_ip::ClientIp::from_request_parts(parts, state).await {
            Ok(axum_client_ip::ClientIp(ip)) => ip,
            Err(_) => {
                // 提取失败时 fallback 到 0.0.0.0（unspecified）而**不是** 127.0.0.1。
                //
                // 用 localhost 做 fallback 是限流领域典型陷阱：所有"提取失败"的请求
                // 共享同一个桶，攻击者只要让 IP 提取失败就能用 1 个桶打爆所有限流；
                // 更糟的是若运维不慎把 `127.0.0.1/32` 加进 allowlist（想给本机内部
                // 调用免限流），所有恶意流量会瞬间被加白。0.0.0.0 不会被任何合理的
                // allowlist 命中，最坏情况下也只是攻击者自己挤占一个独立桶。
                IP_FALLBACK_WARNED.get_or_init(|| {
                    tracing::warn!(
                        "rate-limit: client IP extraction failed; falling back to 0.0.0.0. \
                         This usually means `ClientIpSource` layer is not configured; \
                         see axum-client-ip docs."
                    );
                });
                IpAddr::V4(Ipv4Addr::UNSPECIFIED)
            }
        };

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
        let metadata = if let Some(holder) = parts.extensions.get::<SharedMetadataHolder>().cloned()
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
