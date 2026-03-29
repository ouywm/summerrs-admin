use std::time::Duration;

use sea_orm::ConnectOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DataSourceRole {
    #[default]
    Primary,
    Replica,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceKind {
    #[default]
    RoundRobin,
    Random,
    Weight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceConfig {
    pub uri: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub role: DataSourceRole,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub enable_logging: bool,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default)]
    pub connect_timeout: Option<u64>,
    #[serde(default)]
    pub idle_timeout: Option<u64>,
    #[serde(default)]
    pub acquire_timeout: Option<u64>,
    #[serde(default = "default_test_before_acquire")]
    pub test_before_acquire: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWriteRuleConfig {
    pub name: String,
    pub primary: String,
    #[serde(default)]
    pub replicas: Vec<String>,
    #[serde(default)]
    pub load_balance: LoadBalanceKind,
}

const fn default_weight() -> u32 {
    1
}

const fn default_min_connections() -> u32 {
    1
}

const fn default_max_connections() -> u32 {
    10
}

const fn default_test_before_acquire() -> bool {
    true
}

impl DataSourceConfig {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            schema: None,
            role: DataSourceRole::Primary,
            weight: default_weight(),
            enable_logging: false,
            min_connections: default_min_connections(),
            max_connections: default_max_connections(),
            connect_timeout: None,
            idle_timeout: None,
            acquire_timeout: None,
            test_before_acquire: default_test_before_acquire(),
        }
    }

    pub fn connect_options(&self) -> ConnectOptions {
        let mut opt = ConnectOptions::new(self.uri.clone());
        opt.max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .sqlx_logging(self.enable_logging)
            .test_before_acquire(self.test_before_acquire);

        if let Some(connect_timeout) = self.connect_timeout {
            opt.connect_timeout(Duration::from_millis(connect_timeout));
        }
        if let Some(idle_timeout) = self.idle_timeout {
            opt.idle_timeout(Duration::from_millis(idle_timeout));
        }
        if let Some(acquire_timeout) = self.acquire_timeout {
            opt.acquire_timeout(Duration::from_millis(acquire_timeout));
        }

        opt
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::DataSourceConfig;

    #[test]
    fn datasource_config_defaults_align_with_sea_orm_plugin_style() {
        let config = DataSourceConfig::new("postgres://localhost/app");

        assert!(!config.enable_logging);
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connect_timeout, None);
        assert_eq!(config.idle_timeout, None);
        assert_eq!(config.acquire_timeout, None);
        assert!(config.test_before_acquire);
    }

    #[test]
    fn datasource_config_builds_connect_options_from_pool_fields() {
        let config = DataSourceConfig {
            enable_logging: true,
            min_connections: 2,
            max_connections: 16,
            connect_timeout: Some(1_500),
            idle_timeout: Some(2_500),
            acquire_timeout: Some(3_500),
            test_before_acquire: false,
            ..DataSourceConfig::new("postgres://localhost/app")
        };

        let options = config.connect_options();

        assert_eq!(options.get_url(), "postgres://localhost/app");
        assert_eq!(options.get_min_connections(), Some(2));
        assert_eq!(options.get_max_connections(), Some(16));
        assert_eq!(
            options.get_connect_timeout(),
            Some(Duration::from_millis(1_500))
        );
        assert_eq!(
            options.get_idle_timeout(),
            Some(Some(Duration::from_millis(2_500)))
        );
        assert_eq!(
            options.get_acquire_timeout(),
            Some(Duration::from_millis(3_500))
        );
        assert!(options.get_sqlx_logging());
    }
}
