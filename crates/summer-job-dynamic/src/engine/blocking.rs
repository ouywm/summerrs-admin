//! 阻塞策略实现 —— 同一个 job 上一次还没跑完，新触发到达时怎么处理。
//!
//! 三种策略：
//! - **Discard**：CAS 抢 atomic flag，失败则跳过本次触发（写 DISCARDED 终态）
//! - **Serial**：FIFO 排队，前一次（含 retry）跑完后下一次才能进
//! - **Override**：取消旧执行（通过 [`CancellationToken`]），立即开始新执行
//!
//! 实现细节：
//! - Serial 用 `Arc<tokio::sync::Mutex>` per job_id，靠 tokio Mutex 的 FIFO 唤醒顺序保证排队语义
//! - Override 维护 `HashMap<job_id, CancellationToken>`，新触发到达时 cancel 旧 token；旧 token
//!   的 cancel 通过 `BlockingGuard::Override` 传播到 worker 内部的 `ctx.cancel`，handler
//!   配合 `ctx.check_cancel()` cooperative 退出
//! - Discard 路径不变（保持原 P2.3 行为）

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};
use tokio_util::sync::CancellationToken;

use crate::enums::BlockingStrategy;

/// 全局并发追踪器，所有策略共用一份；跟随 plugin 生命周期。
#[derive(Clone, Default)]
pub struct BlockingTracker {
    /// Discard 用：每个 job_id 一个 atomic flag
    discard: Arc<RwLock<HashMap<i64, Arc<RunningSlot>>>>,
    /// Serial 用：每个 job_id 一个 Mutex，新触发 await lock 排队
    serial: Arc<RwLock<HashMap<i64, Arc<Mutex<()>>>>>,
    /// Override 用：每个 job_id 当前持有的 cancel token；新触发到达时 cancel 旧的
    override_tokens: Arc<RwLock<HashMap<i64, CancellationToken>>>,
}

#[derive(Default)]
pub struct RunningSlot {
    pub flag: AtomicBool,
}

impl BlockingTracker {
    /// 按策略获取执行权。
    /// - Discard 失败 → `Discarded`（worker 写 DISCARDED 终态后退出）
    /// - Serial → await 拿到锁（FIFO 排队）
    /// - Override → 立即获得新 token，旧 token 已被 cancel
    pub async fn acquire(&self, job_id: i64, strategy: BlockingStrategy) -> AcquireResult {
        match strategy {
            BlockingStrategy::Discard => self.acquire_discard(job_id).await,
            BlockingStrategy::Serial => self.acquire_serial(job_id).await,
            BlockingStrategy::Override => self.acquire_override(job_id).await,
        }
    }

    async fn acquire_discard(&self, job_id: i64) -> AcquireResult {
        let slot = self.get_or_create_discard_slot(job_id).await;
        if slot
            .flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            AcquireResult::Acquired(BlockingGuard::Discard(SlotGuard { slot }))
        } else {
            AcquireResult::Discarded
        }
    }

    async fn acquire_serial(&self, job_id: i64) -> AcquireResult {
        let mutex = self.get_or_create_serial_mutex(job_id).await;
        // tokio Mutex 的 wakers 链表按 await 顺序唤醒，是 FIFO 的
        let guard = mutex.lock_owned().await;
        AcquireResult::Acquired(BlockingGuard::Serial(guard))
    }

    async fn acquire_override(&self, job_id: i64) -> AcquireResult {
        let new_token = CancellationToken::new();
        let prev = {
            let mut map = self.override_tokens.write().await;
            map.insert(job_id, new_token.clone())
        };
        if let Some(old) = prev {
            old.cancel();
        }
        AcquireResult::Acquired(BlockingGuard::Override {
            token: new_token,
            tracker: self.clone(),
            job_id,
        })
    }

    async fn get_or_create_discard_slot(&self, job_id: i64) -> Arc<RunningSlot> {
        if let Some(slot) = self.discard.read().await.get(&job_id) {
            return slot.clone();
        }
        let mut map = self.discard.write().await;
        map.entry(job_id)
            .or_insert_with(|| Arc::new(RunningSlot::default()))
            .clone()
    }

    async fn get_or_create_serial_mutex(&self, job_id: i64) -> Arc<Mutex<()>> {
        if let Some(m) = self.serial.read().await.get(&job_id) {
            return m.clone();
        }
        let mut map = self.serial.write().await;
        map.entry(job_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Override 释放当前 token：仅当 map 里的 token 仍是 self 持有的那个时清理，
    /// 避免误删后续 trigger 已经放进去的新 token。
    async fn release_override(&self, job_id: i64, my_token: &CancellationToken) {
        let mut map = self.override_tokens.write().await;
        if let Some(current) = map.get(&job_id)
            && std::ptr::eq(
                current as *const CancellationToken,
                my_token as *const CancellationToken,
            )
        {
            map.remove(&job_id);
        }
    }
}

/// 按策略获得执行权的结果。
pub enum AcquireResult {
    Acquired(BlockingGuard),
    /// Discard 策略下被丢弃；调用方写 DISCARDED 终态后退出
    Discarded,
}

/// 持有期间表示当前 trigger 拥有执行权；drop 时自动释放给下一个等待者（Serial）
/// 或重置 flag（Discard）或清理 map（Override）。
pub enum BlockingGuard {
    Discard(SlotGuard),
    Serial(#[allow(dead_code)] OwnedMutexGuard<()>),
    Override {
        /// 当前 trigger 持有的 cancel token；外部新 trigger 到达会 cancel 它，
        /// worker 应订阅 `cancelled()` 同步取消 ctx。
        token: CancellationToken,
        tracker: BlockingTracker,
        job_id: i64,
    },
}

impl BlockingGuard {
    /// Override 策略下返回 outer cancel token；其他策略返回 None。
    /// worker 用它桥接到 `ctx.cancel`，让 handler 通过 `ctx.check_cancel()` 退出。
    pub fn outer_cancel(&self) -> Option<CancellationToken> {
        match self {
            BlockingGuard::Override { token, .. } => Some(token.clone()),
            _ => None,
        }
    }
}

impl Drop for BlockingGuard {
    fn drop(&mut self) {
        if let BlockingGuard::Override {
            token,
            tracker,
            job_id,
        } = self
        {
            // 异步清理 override map；用 spawn 避免在 sync drop 中阻塞
            let tracker = tracker.clone();
            let token = token.clone();
            let job_id = *job_id;
            tokio::spawn(async move {
                tracker.release_override(job_id, &token).await;
            });
        }
    }
}

/// Discard 策略的 slot guard；drop 时复位 flag。
pub struct SlotGuard {
    slot: Arc<RunningSlot>,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.slot.flag.store(false, Ordering::Release);
    }
}

// -----------------------------------------------------------------------------
// 测试
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    #[tokio::test]
    async fn discard_blocks_concurrent() {
        let tracker = BlockingTracker::default();
        let r1 = tracker.acquire(42, BlockingStrategy::Discard).await;
        let r2 = tracker.acquire(42, BlockingStrategy::Discard).await;
        assert!(matches!(r1, AcquireResult::Acquired(_)));
        assert!(matches!(r2, AcquireResult::Discarded));
        drop(r1);
        let r3 = tracker.acquire(42, BlockingStrategy::Discard).await;
        assert!(matches!(r3, AcquireResult::Acquired(_)));
    }

    #[tokio::test]
    async fn serial_queues_in_fifo_order() {
        let tracker = BlockingTracker::default();
        let g1 = tracker.acquire(7, BlockingStrategy::Serial).await;
        // 启动 2 个 waiter，依次抢锁
        let t1 = tracker.clone();
        let t2 = tracker.clone();
        let h1 = tokio::spawn(async move {
            let g = t1.acquire(7, BlockingStrategy::Serial).await;
            (Instant::now(), g)
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let h2 = tokio::spawn(async move {
            let g = t2.acquire(7, BlockingStrategy::Serial).await;
            (Instant::now(), g)
        });
        // 释放 g1，waiter1 应该先拿到
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(g1);
        let (t1_at, g1_w) = h1.await.unwrap();
        // g1_w 还持有锁，t2 应阻塞
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!h2.is_finished(), "waiter2 should still be blocked");
        drop(g1_w);
        let (t2_at, _) = h2.await.unwrap();
        assert!(t2_at >= t1_at, "FIFO order violated");
    }

    #[tokio::test]
    async fn override_cancels_previous() {
        let tracker = BlockingTracker::default();
        let r1 = tracker.acquire(11, BlockingStrategy::Override).await;
        let token1 = match &r1 {
            AcquireResult::Acquired(g) => g.outer_cancel().unwrap(),
            _ => panic!("expected Acquired"),
        };
        let r2 = tracker.acquire(11, BlockingStrategy::Override).await;
        // r1 持有的 token 应该已被 r2 cancel
        assert!(token1.is_cancelled(), "previous token should be cancelled");
        let _ = r2;
    }
}
