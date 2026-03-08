//! ip2region 配置

use serde::Deserialize;
use summer::config::Configurable;

#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "ip2region"]
pub struct Ip2RegionConfig {
    #[serde(default = "default_ipv4_db_path")]
    pub ipv4_db_path: String,
    pub ipv6_db_path: Option<String>,
}

fn default_ipv4_db_path() -> String {
    "./data/ip2region_v4.xdb".to_string()
}
