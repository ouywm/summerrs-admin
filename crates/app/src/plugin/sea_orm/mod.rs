//! 自定义 SeaORM 插件（基于 sea-orm 2.x），复用 [sea-orm] 配置段

pub mod config;
pub(crate) mod pagination;

pub use config::{SeaOrmConfig, SeaOrmWebConfig};

use anyhow::Context;
use sea_orm::{ConnectOptions, Database};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};
use std::time::Duration;

/// 数据库连接类型别名
pub type DbConn = sea_orm::DatabaseConnection;

/// 自定义 SeaORM 插件
pub struct SeaOrmPlugin;

#[async_trait]
impl Plugin for SeaOrmPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<SeaOrmConfig>()
            .expect("sea-orm plugin config load failed");

        let conn = Self::connect(&config)
            .await
            .expect("sea-orm plugin connect failed");

        app.add_component(conn);
    }

    fn name(&self) -> &str {
        "sea-orm"
    }
}

impl SeaOrmPlugin {
    async fn connect(config: &SeaOrmConfig) -> anyhow::Result<DbConn> {
        let mut opt = ConnectOptions::new(&config.uri);
        opt.max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .sqlx_logging(config.enable_logging);

        if let Some(connect_timeout) = config.connect_timeout {
            opt.connect_timeout(Duration::from_millis(connect_timeout));
        }
        if let Some(idle_timeout) = config.idle_timeout {
            opt.idle_timeout(Duration::from_millis(idle_timeout));
        }
        if let Some(acquire_timeout) = config.acquire_timeout {
            opt.acquire_timeout(Duration::from_millis(acquire_timeout));
        }

        Database::connect(opt)
            .await
            .context(format!("sea-orm connection failed: {}", &config.uri))
    }
}
