//! SummerAiAdminPlugin —— admin 域 Plugin 入口。

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// admin 域 Plugin 入口。
pub struct SummerAiAdminPlugin;

#[async_trait]
impl Plugin for SummerAiAdminPlugin {
    async fn build(&self, _app: &mut AppBuilder) {
        tracing::info!(
            group = Self::admin_group(),
            "summer-ai-admin plugin initialized"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_admin::SummerAiAdminPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![]
    }
}

impl SummerAiAdminPlugin {
    pub fn admin_group() -> &'static str {
        env!("CARGO_PKG_NAME")
    }
}
