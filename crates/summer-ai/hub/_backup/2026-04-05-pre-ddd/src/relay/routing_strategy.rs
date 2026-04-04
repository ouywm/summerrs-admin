//! Pluggable routing strategies for channel selection.
//!
//! The default strategy is `PriorityWeightedRandom` (current behavior):
//!   1. Filter by highest effective priority
//!   2. Filter by best health score
//!   3. Weighted random among ties
//!
//! Additional strategies can be implemented and selected via channel group
//! configuration.

/// Context passed to each strategy invocation.
pub struct RoutingContext<'a> {
    pub model: &'a str,
    pub endpoint_scope: &'a str,
    pub estimated_tokens: i64,
}

/// A candidate channel with its health metrics, ready for strategy selection.
#[derive(Debug, Clone)]
pub struct RouteCandidate {
    pub channel_id: i64,
    pub channel_name: String,
    pub channel_type: i16,
    pub base_url: String,
    pub model_mapping: serde_json::Value,
    pub priority: i32,
    pub weight: i32,
    pub response_time: i32,
    pub failure_streak: i32,
    pub recent_penalty_count: i32,
    pub account_id: i64,
    pub account_name: String,
    pub api_key: String,
}

/// Trait for routing strategies.
pub trait RoutingStrategy: Send + Sync {
    /// Strategy name for logging / config reference.
    fn name(&self) -> &'static str;

    /// Select the best candidate from the given list.
    ///
    /// Returns the index into `candidates`, or `None` if no suitable candidate.
    fn select(&self, candidates: &[RouteCandidate], ctx: &RoutingContext<'_>) -> Option<usize>;
}

// ── Built-in Strategies ────────────────────────────────────────────

/// Default: Priority → Health → Weighted Random (current behavior).
pub struct PriorityWeightedRandom;

impl RoutingStrategy for PriorityWeightedRandom {
    fn name(&self) -> &'static str {
        "priority_weighted_random"
    }

    fn select(&self, candidates: &[RouteCandidate], _ctx: &RoutingContext<'_>) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }

        let max_priority = candidates.iter().map(|c| c.priority).max()?;
        let top: Vec<(usize, &RouteCandidate)> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.priority == max_priority)
            .collect();

        let best_health = top.iter().map(|(_, c)| health_score(c)).min()?;
        let finalists: Vec<(usize, i32)> = top
            .iter()
            .filter(|(_, c)| health_score(c) == best_health)
            .map(|(i, c)| (*i, c.weight.max(1)))
            .collect();

        weighted_random(&finalists)
    }
}

/// Lowest latency: prefer channels with the fastest response time.
pub struct LowestLatency;

impl RoutingStrategy for LowestLatency {
    fn name(&self) -> &'static str {
        "lowest_latency"
    }

    fn select(&self, candidates: &[RouteCandidate], _ctx: &RoutingContext<'_>) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }

        candidates
            .iter()
            .enumerate()
            .min_by_key(|(_, c)| {
                // Penalize unhealthy channels with artificial latency.
                let penalty = c.recent_penalty_count * 1000;
                c.response_time + penalty
            })
            .map(|(i, _)| i)
    }
}

/// Round-robin: cycle through candidates in order.
pub struct RoundRobin {
    counter: std::sync::atomic::AtomicU64,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingStrategy for RoundRobin {
    fn name(&self) -> &'static str {
        "round_robin"
    }

    fn select(&self, candidates: &[RouteCandidate], _ctx: &RoutingContext<'_>) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }

        let index = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some((index as usize) % candidates.len())
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Composite health score — lower is healthier.
fn health_score(c: &RouteCandidate) -> i32 {
    c.failure_streak * 100 + c.recent_penalty_count * 10 + c.recent_penalty_count
}

/// Weighted random selection. Returns the index of the selected item.
fn weighted_random(items: &[(usize, i32)]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    if items.len() == 1 {
        return Some(items[0].0);
    }

    let total_weight: i32 = items.iter().map(|(_, w)| *w).sum();
    if total_weight <= 0 {
        return Some(items[0].0);
    }

    let mut rng = rand::rng();
    let mut roll = rand::RngExt::random_range(&mut rng, 0..total_weight);

    for &(index, weight) in items {
        roll -= weight;
        if roll < 0 {
            return Some(index);
        }
    }

    Some(items.last()?.0)
}

/// Look up a strategy by name.
pub fn strategy_by_name(name: &str) -> Box<dyn RoutingStrategy> {
    match name {
        "lowest_latency" => Box::new(LowestLatency),
        "round_robin" => Box::new(RoundRobin::new()),
        _ => Box::new(PriorityWeightedRandom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidates() -> Vec<RouteCandidate> {
        vec![
            RouteCandidate {
                channel_id: 1,
                channel_name: "fast".into(),
                channel_type: 1,
                base_url: "http://a".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 5,
                response_time: 50,
                failure_streak: 0,
                recent_penalty_count: 0,
                account_id: 101,
                account_name: "acc-a".into(),
                api_key: "key-a".into(),
            },
            RouteCandidate {
                channel_id: 2,
                channel_name: "slow".into(),
                channel_type: 1,
                base_url: "http://b".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 5,
                response_time: 500,
                failure_streak: 0,
                recent_penalty_count: 0,
                account_id: 102,
                account_name: "acc-b".into(),
                api_key: "key-b".into(),
            },
        ]
    }

    fn ctx() -> RoutingContext<'static> {
        RoutingContext {
            model: "gpt-4o",
            endpoint_scope: "chat",
            estimated_tokens: 100,
        }
    }

    #[test]
    fn lowest_latency_picks_fastest() {
        let candidates = make_candidates();
        let strategy = LowestLatency;
        let selected = strategy.select(&candidates, &ctx()).unwrap();
        assert_eq!(selected, 0); // "fast" channel (50ms)
    }

    #[test]
    fn round_robin_cycles() {
        let candidates = make_candidates();
        let strategy = RoundRobin::new();
        let ctx = ctx();

        let a = strategy.select(&candidates, &ctx).unwrap();
        let b = strategy.select(&candidates, &ctx).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn strategy_by_name_returns_correct_type() {
        assert_eq!(strategy_by_name("lowest_latency").name(), "lowest_latency");
        assert_eq!(strategy_by_name("round_robin").name(), "round_robin");
        assert_eq!(
            strategy_by_name("unknown").name(),
            "priority_weighted_random"
        );
    }

    #[test]
    fn empty_candidates_returns_none() {
        let strategy = PriorityWeightedRandom;
        assert!(strategy.select(&[], &ctx()).is_none());
    }
}
