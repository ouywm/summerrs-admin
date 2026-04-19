//! SummerAiAdminPlugin —— 后台管理域 Plugin 入口。
//!
//! 负责：
//! - 挂 `/admin/ai/*` 路由（CRUD：channel / channel_account / price / token / ...）
//! - 管理员鉴权（复用 summer-auth 的 session）

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// admin 域 Plugin 入口。
pub struct SummerAiAdminPlugin;

#[async_trait]
impl Plugin for SummerAiAdminPlugin {
    async fn build(&self, _app: &mut AppBuilder) {
        tracing::info!("summer-ai-admin plugin initialized (P0 skeleton)");
    }

    fn name(&self) -> &str {
        "summer_ai_admin::SummerAiAdminPlugin"
    }
}
