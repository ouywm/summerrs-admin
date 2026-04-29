//! SummerAiRelayPlugin —— relay 域 Plugin 入口。
//!
//! 职责：
//! - 注册 `reqwest::Client`（出站 HTTP 客户端）作 component
//! - 构造 `AiTokenStore` 作 component
//! - 收集本 crate 的路由并配置认证中间件
//!
//! `ChannelStore` / `TrackingService` 都用 `#[derive(Service)]`，summer 框架的
//! `auto_inject_service` 会在 app 启动时自动从 inventory 里把它们装进 registry,
//! 并按 `#[inject(component)]` 字段拉取依赖（`DbConn` / `Redis` / `BackgroundTaskQueue`）。
//!
//! handler 通过 `#[post("/...")]` 宏自动注册到 inventory，
//! 路由通过 `router::router()` 收集并在 main.rs 中挂载。
//!
//! 依赖 `summer_sea_orm::SeaOrmPlugin` + `summer_redis::RedisPlugin` +
//! `summer_plugins::BackgroundTaskPlugin`（注入源）+ `summer_auth::SummerAuthPlugin`（认证）。

use std::time::Duration;

use crate::auth::AiTokenStore;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;
use summer_sea_orm::DbConn;

/// relay 域 Plugin 入口。
pub struct SummerAiRelayPlugin;

#[async_trait]
impl Plugin for SummerAiRelayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(?error, "summer-ai-relay reqwest client build failed");
                return;
            }
        };
        app.add_component(http);

        let db = app
            .get_component::<DbConn>()
            .expect("summer-ai-relay requires DbConn (SeaOrmPlugin)");
        let redis = app
            .get_component::<Redis>()
            .expect("summer-ai-relay requires Redis (RedisPlugin)");

        let token_store = AiTokenStore::new(db, redis);
        app.add_component(token_store);

        tracing::info!(
            group = crate::relay_group(),
            "summer-ai-relay plugin initialized (ApiKeyStrategy bound)"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin", "summer_redis::RedisPlugin"]
    }
}
