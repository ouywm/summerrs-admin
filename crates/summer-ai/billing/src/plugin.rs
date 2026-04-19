//! SummerAiBillingPlugin —— 计费域 Plugin 入口。
//!
//! 负责：
//! - 注册 BillingEngine 为 Component
//! - 提供 reserve / settle / refund API
//! - 后台任务：对账 / usage cleanup

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// billing 域 Plugin 入口。
pub struct SummerAiBillingPlugin;

#[async_trait]
impl Plugin for SummerAiBillingPlugin {
    async fn build(&self, _app: &mut AppBuilder) {
        tracing::info!("summer-ai-billing plugin initialized (P0 skeleton)");
    }

    fn name(&self) -> &str {
        "summer_ai_billing::SummerAiBillingPlugin"
    }
}
