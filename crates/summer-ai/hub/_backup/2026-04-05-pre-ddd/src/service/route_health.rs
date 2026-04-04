use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::runtime_cache::RuntimeCacheService;

const ROUTE_HEALTH_TTL_SECONDS: u64 = 5 * 60;
const ROUTE_HEALTH_FIELD_PENALTY: &str = "recent_penalty_count";
const ROUTE_HEALTH_FIELD_RATE_LIMIT: &str = "recent_rate_limit_count";
const ROUTE_HEALTH_FIELD_OVERLOAD: &str = "recent_overload_count";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteHealthSnapshot {
    pub recent_penalty_count: i32,
    pub recent_rate_limit_count: i32,
    pub recent_overload_count: i32,
}

/// Result of a batch health snapshot load.
pub struct BatchHealthSnapshots {
    pub accounts: std::collections::HashMap<i64, RouteHealthSnapshot>,
    pub channels: std::collections::HashMap<i64, RouteHealthSnapshot>,
}

impl RouteHealthSnapshot {
    fn is_empty(&self) -> bool {
        self.recent_penalty_count <= 0
            && self.recent_rate_limit_count <= 0
            && self.recent_overload_count <= 0
    }
}

#[derive(Clone, Service)]
pub struct RouteHealthService {
    #[inject(component)]
    cache: RuntimeCacheService,
}

impl RouteHealthService {
    pub fn new(cache: RuntimeCacheService) -> Self {
        Self { cache }
    }

    pub async fn load_channel_snapshot(&self, channel_id: i64) -> ApiResult<RouteHealthSnapshot> {
        self.load_snapshot(&channel_key(channel_id), &legacy_channel_key(channel_id))
            .await
    }

    pub async fn load_account_snapshot(&self, account_id: i64) -> ApiResult<RouteHealthSnapshot> {
        self.load_snapshot(&account_key(account_id), &legacy_account_key(account_id))
            .await
    }

    /// Batch-load health snapshots for multiple accounts and channels in a
    /// single Redis pipeline, eliminating the N+1 per-account query pattern.
    pub async fn batch_load_snapshots(
        &self,
        account_ids: &[i64],
        channel_ids: &[i64],
    ) -> ApiResult<BatchHealthSnapshots> {
        use summer_redis::redis;

        let mut conn = self.cache.connection();
        let mut pipe = redis::pipe();
        for &id in account_ids {
            pipe.cmd("HGETALL").arg(account_key(id));
        }
        for &id in channel_ids {
            pipe.cmd("HGETALL").arg(channel_key(id));
        }

        let results: Vec<std::collections::HashMap<String, i64>> = pipe
            .query_async(&mut conn)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("batch health snapshot: {e}")))?;

        let mut account_snapshots = std::collections::HashMap::new();
        for (i, id) in account_ids.iter().enumerate() {
            let snapshot = results
                .get(i)
                .map(snapshot_from_hash_entries)
                .unwrap_or_default();
            account_snapshots.insert(*id, snapshot);
        }

        let offset = account_ids.len();
        let mut channel_snapshots = std::collections::HashMap::new();
        for (i, id) in channel_ids.iter().enumerate() {
            let snapshot = results
                .get(offset + i)
                .map(snapshot_from_hash_entries)
                .unwrap_or_default();
            channel_snapshots.insert(*id, snapshot);
        }

        Ok(BatchHealthSnapshots {
            accounts: account_snapshots,
            channels: channel_snapshots,
        })
    }

    pub async fn record_relay_failure(
        &self,
        channel_id: i64,
        account_id: i64,
        status_code: i32,
    ) -> ApiResult<bool> {
        if !should_track_route_penalty(status_code) {
            return Ok(false);
        }
        let deltas = relay_failure_deltas(status_code);

        let channel_changed = self
            .mutate_snapshot(&channel_key(channel_id), &deltas)
            .await?;
        let account_changed = self
            .mutate_snapshot(&account_key(account_id), &deltas)
            .await?;

        Ok(channel_changed || account_changed)
    }

    pub async fn record_relay_success(&self, channel_id: i64, account_id: i64) -> ApiResult<bool> {
        let deltas = success_deltas();
        let channel_changed = self
            .mutate_snapshot(&channel_key(channel_id), &deltas)
            .await?;
        let account_changed = self
            .mutate_snapshot(&account_key(account_id), &deltas)
            .await?;

        Ok(channel_changed || account_changed)
    }

    async fn load_snapshot(&self, key: &str, legacy_key: &str) -> ApiResult<RouteHealthSnapshot> {
        let snapshot = snapshot_from_hash_entries(&self.cache.hash_get_all_i64(key).await?);
        if !snapshot.is_empty() {
            return Ok(snapshot);
        }

        self.cache
            .get_json::<RouteHealthSnapshot>(legacy_key)
            .await
            .map(|snapshot| snapshot.unwrap_or_default())
    }

    async fn mutate_snapshot(&self, key: &str, deltas: &[(&str, i64)]) -> ApiResult<bool> {
        self.cache
            .mutate_hash_counters(key, ROUTE_HEALTH_TTL_SECONDS, deltas)
            .await
            .map(|result| result.changed)
    }
}

fn channel_key(channel_id: i64) -> String {
    format!("ai:route-health:v2:channel:{channel_id}")
}

fn account_key(account_id: i64) -> String {
    format!("ai:route-health:v2:account:{account_id}")
}

fn legacy_channel_key(channel_id: i64) -> String {
    format!("ai:route-health:channel:{channel_id}")
}

fn legacy_account_key(account_id: i64) -> String {
    format!("ai:route-health:account:{account_id}")
}

fn should_track_route_penalty(status_code: i32) -> bool {
    matches!(status_code, 0 | 408 | 409 | 425 | 429 | 500..=599)
}

fn relay_failure_deltas(status_code: i32) -> [(&'static str, i64); 3] {
    [
        (ROUTE_HEALTH_FIELD_PENALTY, 1),
        (
            ROUTE_HEALTH_FIELD_RATE_LIMIT,
            i64::from((status_code == 429) as i32),
        ),
        (
            ROUTE_HEALTH_FIELD_OVERLOAD,
            i64::from(matches!(status_code, 500..=599) as i32),
        ),
    ]
}

fn success_deltas() -> [(&'static str, i64); 3] {
    [
        (ROUTE_HEALTH_FIELD_PENALTY, -1),
        (ROUTE_HEALTH_FIELD_RATE_LIMIT, -1),
        (ROUTE_HEALTH_FIELD_OVERLOAD, -1),
    ]
}

fn snapshot_from_hash_entries(
    entries: &std::collections::HashMap<String, i64>,
) -> RouteHealthSnapshot {
    RouteHealthSnapshot {
        recent_penalty_count: entries
            .get(ROUTE_HEALTH_FIELD_PENALTY)
            .copied()
            .unwrap_or_default() as i32,
        recent_rate_limit_count: entries
            .get(ROUTE_HEALTH_FIELD_RATE_LIMIT)
            .copied()
            .unwrap_or_default() as i32,
        recent_overload_count: entries
            .get(ROUTE_HEALTH_FIELD_OVERLOAD)
            .copied()
            .unwrap_or_default() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::try_join_all;
    use summer_redis::redis::AsyncCommands;

    const TEST_REDIS_URL: &str = "redis://127.0.0.1/";

    #[test]
    fn route_health_snapshot_is_empty_when_all_recent_counters_are_zero() {
        assert!(RouteHealthSnapshot::default().is_empty());
        assert!(
            !RouteHealthSnapshot {
                recent_penalty_count: 1,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
            }
            .is_empty()
        );
    }

    #[test]
    fn should_track_route_penalty_matches_retryable_upstream_failures() {
        assert!(should_track_route_penalty(0));
        assert!(should_track_route_penalty(429));
        assert!(should_track_route_penalty(503));
        assert!(!should_track_route_penalty(400));
        assert!(!should_track_route_penalty(401));
    }

    #[test]
    fn snapshot_from_hash_entries_maps_missing_fields_to_zero() {
        let snapshot = snapshot_from_hash_entries(&std::collections::HashMap::from([
            (ROUTE_HEALTH_FIELD_PENALTY.to_string(), 2),
            (ROUTE_HEALTH_FIELD_RATE_LIMIT.to_string(), 1),
        ]));

        assert_eq!(
            snapshot,
            RouteHealthSnapshot {
                recent_penalty_count: 2,
                recent_rate_limit_count: 1,
                recent_overload_count: 0,
            }
        );
    }

    async fn test_service() -> (RouteHealthService, RuntimeCacheService, summer_redis::Redis) {
        let redis = summer_redis::redis::Client::open(
            std::env::var("REDIS_URL").unwrap_or_else(|_| TEST_REDIS_URL.to_string()),
        )
        .expect("create redis client")
        .get_connection_manager()
        .await
        .expect("connect redis");
        let cache = RuntimeCacheService::new(redis.clone());
        (RouteHealthService::new(cache.clone()), cache, redis)
    }

    async fn ttl(redis: &summer_redis::Redis, key: &str) -> i64 {
        let mut conn = redis.clone();
        conn.ttl(key).await.expect("query ttl")
    }

    #[tokio::test]
    #[ignore = "requires local redis"]
    async fn record_relay_failure_preserves_all_concurrent_penalty_updates() {
        let (service, cache, redis) = test_service().await;
        let channel_id = 601;
        let account_id = 701;
        let channel_key = channel_key(channel_id);
        let account_key = account_key(account_id);

        cache
            .delete(&channel_key)
            .await
            .expect("cleanup channel key");
        cache
            .delete(&account_key)
            .await
            .expect("cleanup account key");

        try_join_all((0..32).map(|_| {
            let service = service.clone();
            async move {
                service
                    .record_relay_failure(channel_id, account_id, 429)
                    .await
            }
        }))
        .await
        .expect("record concurrent failures");

        assert_eq!(
            service
                .load_channel_snapshot(channel_id)
                .await
                .expect("load channel snapshot"),
            RouteHealthSnapshot {
                recent_penalty_count: 32,
                recent_rate_limit_count: 32,
                recent_overload_count: 0,
            }
        );
        assert_eq!(
            service
                .load_account_snapshot(account_id)
                .await
                .expect("load account snapshot"),
            RouteHealthSnapshot {
                recent_penalty_count: 32,
                recent_rate_limit_count: 32,
                recent_overload_count: 0,
            }
        );
        assert!(
            ttl(&redis, &channel_key).await > 0,
            "channel hash should have ttl"
        );
        assert!(
            ttl(&redis, &account_key).await > 0,
            "account hash should have ttl"
        );

        cache
            .delete(&channel_key)
            .await
            .expect("delete channel key");
        cache
            .delete(&account_key)
            .await
            .expect("delete account key");
    }

    #[tokio::test]
    #[ignore = "requires local redis"]
    async fn record_relay_success_saturates_and_clears_empty_snapshot() {
        let (service, cache, redis) = test_service().await;
        let channel_id = 602;
        let account_id = 702;
        let channel_key = channel_key(channel_id);
        let account_key = account_key(account_id);

        cache
            .mutate_hash_counters(
                &channel_key,
                ROUTE_HEALTH_TTL_SECONDS,
                &[
                    (ROUTE_HEALTH_FIELD_PENALTY, 1),
                    (ROUTE_HEALTH_FIELD_RATE_LIMIT, 1),
                    (ROUTE_HEALTH_FIELD_OVERLOAD, 1),
                ],
            )
            .await
            .expect("seed channel snapshot");
        cache
            .mutate_hash_counters(
                &account_key,
                ROUTE_HEALTH_TTL_SECONDS,
                &[(ROUTE_HEALTH_FIELD_PENALTY, 1)],
            )
            .await
            .expect("seed account snapshot");

        assert!(ttl(&redis, &channel_key).await > 0, "seed channel ttl");
        assert!(ttl(&redis, &account_key).await > 0, "seed account ttl");

        assert!(
            service
                .record_relay_success(channel_id, account_id)
                .await
                .expect("record success")
        );
        assert_eq!(
            service
                .load_channel_snapshot(channel_id)
                .await
                .expect("load channel snapshot"),
            RouteHealthSnapshot::default()
        );
        assert_eq!(
            service
                .load_account_snapshot(account_id)
                .await
                .expect("load account snapshot"),
            RouteHealthSnapshot::default()
        );

        let mut conn = redis.clone();
        let channel_exists: bool = conn.exists(&channel_key).await.expect("channel exists");
        let account_exists: bool = conn.exists(&account_key).await.expect("account exists");
        assert!(!channel_exists, "channel hash should be cleared");
        assert!(!account_exists, "account hash should be cleared");
    }
}
