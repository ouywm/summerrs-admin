//! Key Picker —— 从 account 的 `enabled_api_keys()` 里选一个 key。
//!
//! # 为什么独立一个 trait
//!
//! 选 key 跟选 channel/account 不是同一个语义层：
//! - **选 account** 由 `(priority, weight)` 加权随机决定（一组账号里挑一个）
//! - **选 key** 是 account 内部的 "round-robin / random / trace-sticky"
//!
//! 把选 key 抽成 trait 的好处：
//! - 以后接 [Rendezvous hashing](https://en.wikipedia.org/wiki/Rendezvous_hashing) +
//!   trace-sticky LRU（参考 axonhub `TraceStickyKeyProvider`），只需加一个 `impl`
//! - 测试里塞 deterministic picker（固定返首个 key），不用 mock `rand`
//!
//! # 当前实现
//!
//! - [`RandomKeyPicker`]：纯随机（fastrand），零状态、零锁。一个 account 里的 keys 数量
//!   通常个位数，随机跟 round-robin 的分布差异可忽略。后续如果需要"公平轮询"再加
//!   `RoundRobinKeyPicker`（带 atomic counter）。

use rand::seq::IndexedRandom;

/// 选 key 策略抽象。
///
/// `keys` 保证**非空**由调用方（`ChannelStore::pick`）维护；实现返 `None` 表示
/// "我也不想选"（目前没有这种场景）。
pub trait KeyPicker: Send + Sync {
    fn pick<'a>(&self, keys: &'a [String]) -> Option<&'a str>;
}

/// 纯随机选 key。
#[derive(Debug, Clone, Copy, Default)]
pub struct RandomKeyPicker;

impl KeyPicker for RandomKeyPicker {
    fn pick<'a>(&self, keys: &'a [String]) -> Option<&'a str> {
        let mut rng = rand::rng();
        keys.choose(&mut rng).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn random_pick_empty_returns_none() {
        let keys: Vec<String> = Vec::new();
        assert!(RandomKeyPicker.pick(&keys).is_none());
    }

    #[test]
    fn random_pick_single_key_always_returns_it() {
        let keys = vec!["sk-only".to_string()];
        for _ in 0..20 {
            assert_eq!(RandomKeyPicker.pick(&keys), Some("sk-only"));
        }
    }

    #[test]
    fn random_pick_covers_all_keys_over_many_iterations() {
        // 概率性测试：200 轮里 3 个 key 都应该被选到（概率极高）
        let keys: Vec<String> = (1..=3).map(|i| format!("sk-{i}")).collect();
        let mut seen: HashSet<String> = HashSet::new();
        for _ in 0..200 {
            if let Some(k) = RandomKeyPicker.pick(&keys) {
                seen.insert(k.to_string());
            }
        }
        assert_eq!(seen.len(), 3, "expected all 3 keys to appear, got {seen:?}");
    }
}
