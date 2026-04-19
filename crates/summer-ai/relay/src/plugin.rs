//! SummerAiRelayPlugin —— relay 域 Plugin 入口。
//!
//! 职责：
//! - 注册 `reqwest::Client`（出站 HTTP 客户端）
//! - 构造 `AiTokenStore` 并挂 `AiAuthLayer` 到 relay 路由（只作用于 `/v1/*` 和
//!   `/v1beta/*`，通过 `relay_router().layer(...)` 局部挂载实现）
//! - 挂载 `/v1/*` 与 `/v1beta/models/*` 路由
//!
//! `ChannelStore` 通过 `#[derive(Service)]` 自动从 Component registry 注入，
//! 不需要在这里手动构造。
//!
//! 依赖 `summer_sea_orm::SeaOrmPlugin` + `summer_redis::RedisPlugin`（注入源）。

use std::time::Duration;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;
use summer_sea_orm::DbConn;
use summer_web::LayerConfigurator;

use crate::auth::{AiAuthLayer, AiTokenStore};
use crate::router::relay_router;

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
        app.add_component(token_store.clone());

        let auth_layer = AiAuthLayer::new(token_store);
        app.add_router_layer(move |router| router.merge(relay_router().layer(auth_layer.clone())));

        tracing::info!("summer-ai-relay plugin initialized (P5 auth layer attached)");
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin", "summer_redis::RedisPlugin"]
    }
}
