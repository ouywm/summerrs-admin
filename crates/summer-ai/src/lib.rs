//! summer-ai
//!
//! LLM 中转网关主 crate —— 聚合 5 个 sub-crate Plugin 的门面。
//!
//! # 使用
//!
//! ```ignore
//! app.add_plugin(summer_ai::SummerAiPlugin);
//! ```
//!
//! # 子 crate
//!
//! - [`summer_ai_core`] — 协议层（canonical 类型 + Adapter trait + 21 adapter 实现）
//! - [`summer_ai_model`] — DB Entity（SeaORM）
//! - [`summer_ai_relay`] — 运行时（/v1/* 路由 + 鉴权 + 计费前置）
//! - [`summer_ai_admin`] — 后台（/admin/ai/* CRUD）
//! - [`summer_ai_billing`] — 计费引擎

pub use summer_ai_admin::SummerAiAdminPlugin;
pub use summer_ai_billing::SummerAiBillingPlugin;
pub use summer_ai_core;
pub use summer_ai_model;
pub use summer_ai_relay::SummerAiRelayPlugin;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// summer-ai 总门面 Plugin。一次注册，同时启用 relay / admin / billing。
pub struct SummerAiPlugin;

#[async_trait]
impl Plugin for SummerAiPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        SummerAiRelayPlugin.build(app).await;
        SummerAiAdminPlugin.build(app).await;
        SummerAiBillingPlugin.build(app).await;
        tracing::info!("summer-ai meta plugin initialized (P0 skeleton)");
    }

    fn name(&self) -> &str {
        "summer_ai::SummerAiPlugin"
    }
}
