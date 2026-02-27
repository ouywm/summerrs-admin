//! 自定义 SeaORM 插件（基于 sea-orm 2.x），复用 [sea-orm] 配置段

use anyhow::Context;
use sea_orm::{ConnectOptions, Database};
use serde::Deserialize;
use spring::app::AppBuilder;
use spring::async_trait;
use spring::config::{ConfigRegistry, Configurable};
use spring::plugin::{MutableComponentRegistry, Plugin};
use std::time::Duration;

/// 数据库连接类型别名
pub type DbConn = sea_orm::DatabaseConnection;

/// 复用 spring-sea-orm 的 [sea-orm] 配置格式
#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "sea-orm"]
pub struct SeaOrmConfig {
    pub uri: String,
    #[serde(default)]
    pub enable_logging: bool,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    pub connect_timeout: Option<u64>,
    pub idle_timeout: Option<u64>,
    pub acquire_timeout: Option<u64>,
}

fn default_min_connections() -> u32 {
    1
}
fn default_max_connections() -> u32 {
    10
}

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
