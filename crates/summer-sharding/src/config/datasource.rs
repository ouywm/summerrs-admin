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
