use chrono::{DateTime, FixedOffset};
use summer::plugin::Service;
use summer_common::error::ApiResult;
use uuid::Uuid;

use crate::service::runtime_cache::RuntimeCacheService;

const RUNTIME_OPERATION_RETENTION_SECONDS: i64 = 2 * 24 * 60 * 60;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeOperationalCounts {
    pub retry_count: i64,
    pub fallback_count: i64,
    pub refund_count: i64,
    pub settlement_failure_count: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeOperationalSummary {
    pub total: RuntimeOperationalCounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeOperationalEvent {
    Retry,
    Fallback,
    Refund,
    SettlementFailure,
}

#[derive(Clone, Service)]
pub struct RuntimeOpsService {
    #[inject(component)]
    cache: RuntimeCacheService,
}

impl RuntimeOpsService {
    pub fn new(cache: RuntimeCacheService) -> Self {
        Self { cache }
    }

    pub async fn record_retry(&self) -> ApiResult<()> {
        self.record(RuntimeOperationalEvent::Retry).await
    }

    pub fn record_retry_async(&self) {
        self.record_async(RuntimeOperationalEvent::Retry);
    }

    pub async fn record_fallback(&self) -> ApiResult<()> {
        self.record(RuntimeOperationalEvent::Fallback).await
    }

    pub fn record_fallback_async(&self) {
        self.record_async(RuntimeOperationalEvent::Fallback);
    }

    pub async fn record_refund(&self) -> ApiResult<()> {
        self.record(RuntimeOperationalEvent::Refund).await
    }

    pub fn record_refund_async(&self) {
        self.record_async(RuntimeOperationalEvent::Refund);
    }

    pub async fn record_settlement_failure(&self) -> ApiResult<()> {
        self.record(RuntimeOperationalEvent::SettlementFailure)
            .await
    }

    pub fn record_settlement_failure_async(&self) {
        self.record_async(RuntimeOperationalEvent::SettlementFailure);
    }

    pub async fn summary(
        &self,
        window_start: DateTime<FixedOffset>,
        window_end: DateTime<FixedOffset>,
    ) -> ApiResult<RuntimeOperationalSummary> {
        let start_ms = window_start.timestamp_millis();
        let end_ms = window_end.timestamp_millis();

        Ok(RuntimeOperationalSummary {
            total: RuntimeOperationalCounts {
                retry_count: self
                    .count(RuntimeOperationalEvent::Retry, start_ms, end_ms)
                    .await?,
                fallback_count: self
                    .count(RuntimeOperationalEvent::Fallback, start_ms, end_ms)
                    .await?,
                refund_count: self
                    .count(RuntimeOperationalEvent::Refund, start_ms, end_ms)
                    .await?,
                settlement_failure_count: self
                    .count(RuntimeOperationalEvent::SettlementFailure, start_ms, end_ms)
                    .await?,
            },
        })
    }

    fn record_async(&self, event: RuntimeOperationalEvent) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(error) = this.record(event).await {
                tracing::warn!("failed to record AI runtime operational event: {error}");
            }
        });
    }

    async fn record(&self, event: RuntimeOperationalEvent) -> ApiResult<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let retention_floor = now_ms - RUNTIME_OPERATION_RETENTION_SECONDS * 1000;
        let key = runtime_operation_key(event);
        let member = format!("{now_ms}:{}", Uuid::new_v4());

        self.cache.sorted_set_add(&key, now_ms, &member).await?;
        let _ = self
            .cache
            .sorted_set_remove_by_score(&key, i64::MIN, retention_floor)
            .await?;
        self.cache
            .expire(&key, RUNTIME_OPERATION_RETENTION_SECONDS)
            .await
    }

    async fn count(
        &self,
        event: RuntimeOperationalEvent,
        window_start_ms: i64,
        window_end_ms: i64,
    ) -> ApiResult<i64> {
        self.cache
            .sorted_set_count_by_score(
                &runtime_operation_key(event),
                window_start_ms,
                window_end_ms,
            )
            .await
    }
}

fn runtime_operation_key(event: RuntimeOperationalEvent) -> String {
    let name = match event {
        RuntimeOperationalEvent::Retry => "retry",
        RuntimeOperationalEvent::Fallback => "fallback",
        RuntimeOperationalEvent::Refund => "refund",
        RuntimeOperationalEvent::SettlementFailure => "settlement_failure",
    };
    format!("ai:runtime:ops:{name}")
}
