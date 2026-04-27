//! SummerAiBillingPlugin —— 计费域 Plugin 入口。
//!
//! 职责（随 Phase 递进填充）：
//!
//! - **当前（P5）**：起 `ai.transaction` 批量收集器（`LogBatchCollector`），供未来
//!   `BillingService` 的 reserve/settle/refund 路径非阻塞 INSERT。
//! - **P6**：注册 `BillingService`（reserve/settle/refund）+ `PriceResolver` +
//!   `GroupRatioCache`。
//! - **P9+**：对账 / balance 同步 / 定期 usage cleanup 后台任务。
//!
//! # 为什么用批量收集器
//!
//! `ai.transaction` 是**纯单表、无 FK 依赖**的账务流水。典型消费场景下每次请求写 1-2 条
//! transaction（reserve 一条 + settle 一条），峰值 TPS 高。批量 `insert_many`
//! 比一条一条 INSERT 快 10-30x，又有 500ms 的 flush 兜底，丢数据窗口可控。
//!
//! 与 `ai.request` / `ai.log` 形成鲜明对比：后者三表有 FK 顺序依赖，无法批量，走
//! `TrackingService` + `BackgroundTaskQueue`（see `summer-ai-relay`）。

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::Plugin;
use summer_ai_model::entity::billing::transaction;
use summer_plugins::log_batch_collector::{LogBatchConfig, spawn_typed_collector};

/// billing 域 Plugin 入口。
pub struct SummerAiBillingPlugin;

#[async_trait]
impl Plugin for SummerAiBillingPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        // ai.transaction 批量收集器 —— `LogBatchCollector<transaction::ActiveModel>`
        // 作为 Component 注入到 registry，供 BillingService 用 `#[inject(component)]`
        // 拿到；非阻塞 push，后台线程攒批 insert_many。
        //
        // 配置优先从 `[log-batch]` TOML 拿；缺省走默认（batch=50 / flush=500ms / cap=4096）。
        let config = app
            .get_config::<LogBatchConfig>()
            .unwrap_or_else(|_| LogBatchConfig::default());

        spawn_typed_collector::<transaction::ActiveModel>(app, "ai.transaction", config);

        tracing::info!(
            "summer-ai-billing plugin initialized \
             (transaction batch collector ready; billing engine pending P6)"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_billing::SummerAiBillingPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}
