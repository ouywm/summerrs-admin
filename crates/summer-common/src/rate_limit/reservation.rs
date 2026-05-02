//! 配额预扣的 RAII 凭证。

use crate::rate_limit::config::RateLimitConfig;
use crate::rate_limit::engine::RateLimitEngine;

/// 配额预扣的 RAII 凭证。
///
/// 必须显式调用 [`Self::commit`] 或 [`Self::release`]，否则 Drop 时会
/// **异步退还全部预扣**（避免泄露配额）并在日志里 warn。
///
/// Drop 退还要求 reservation 创建在 tokio runtime 内（构造时缓存了一份
/// `tokio::runtime::Handle`，Drop 时优先使用它 spawn 退还任务，回退到
/// `Handle::try_current` 才使用当前线程的 handle）。在完全没有 tokio
/// runtime 的环境（极罕见）下退还会被丢弃，并记 warn。
#[must_use = "Reservation must be consumed via commit() or release(); \
              dropping will refund everything (with a warning) on a best-effort basis"]
pub struct Reservation {
    pub(crate) engine: RateLimitEngine,
    pub(crate) state: Option<ReservationState>,
    /// 构造时捕获的 tokio handle，用于 Drop 时 spawn 退还。
    pub(crate) handle: Option<tokio::runtime::Handle>,
}

#[derive(Debug)]
pub(crate) struct ReservationState {
    pub(crate) key: String,
    pub(crate) config: RateLimitConfig,
    pub(crate) reserved_cost: u32,
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
        let Some(state) = self.state.take() else {
            return;
        };
        tracing::warn!(
            key = %state.key,
            cost = state.reserved_cost,
            "Reservation dropped without commit/release; auto-refunding"
        );

        // 优先用构造时捕获的 handle；否则尝试当前线程的 handle；都失败则丢弃 + warn。
        let handle = self
            .handle
            .clone()
            .or_else(|| tokio::runtime::Handle::try_current().ok());

        match handle {
            Some(handle) => {
                let engine = self.engine.clone();
                handle.spawn(async move {
                    engine
                        .refund(&state.key, &state.config, state.reserved_cost)
                        .await;
                });
            }
            None => {
                tracing::warn!(
                    key = %state.key,
                    cost = state.reserved_cost,
                    "Reservation dropped outside tokio runtime; refund LOST. \
                     Always create reservations from within a tokio runtime."
                );
            }
        }
    }
}
