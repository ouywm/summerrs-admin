//! ip2region 插件：通过 IP 地址查询地理位置

use std::net::IpAddr;
use std::sync::Arc;

use ip2region::{CachePolicy, Searcher};
use serde::Deserialize;
use spring::app::AppBuilder;
use spring::async_trait;
use spring::config::{ConfigRegistry, Configurable};
use spring::plugin::{MutableComponentRegistry, Plugin};

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

struct Ip2RegionInner {
    ipv4: Searcher,
    ipv6: Option<Searcher>,
}

/// ip2region Searcher 包装类型，支持 IPv4 和可选的 IPv6 查询
#[derive(Clone)]
pub struct Ip2RegionSearcher(Arc<Ip2RegionInner>);

impl Ip2RegionSearcher {
    /// 根据 IP 地址查询地理位置（格式化后的：国家 省份 城市）
    pub fn search_location(&self, ip: &IpAddr) -> String {
        let region = self.search_region(ip);
        if region.is_empty() {
            return region;
        }
        // ip2region 返回格式: "国家|省份|城市|ISP|国家代码"
        // 提取前三段（国家、省份、城市），过滤掉无意义值
        let parts: Vec<&str> = region
            .split('|')
            .take(3)
            .filter(|s| !s.is_empty() && *s != "0" && *s != "Reserved")
            .collect();
        if parts.is_empty() {
            return "内网IP".to_string();
        }
        parts.join(" ")
    }

    /// 根据 IP 地址查询原始 region 字符串
    pub fn search_region(&self, ip: &IpAddr) -> String {
        let ip_str = ip.to_string();
        let result = match ip {
            IpAddr::V4(_) => self.0.ipv4.search(ip_str.as_str()),
            IpAddr::V6(_) => match &self.0.ipv6 {
                Some(searcher) => searcher.search(ip_str.as_str()),
                None => {
                    tracing::debug!("未配置 IPv6 数据库，跳过 IPv6 地址查询: {}", ip);
                    return String::new();
                }
            },
        };
        match result {
            Ok(region) => region,
            Err(e) => {
                tracing::warn!("ip2region 查询失败 {}: {}", ip, e);
                String::new()
            }
        }
    }
}

pub struct Ip2RegionPlugin;

#[async_trait]
impl Plugin for Ip2RegionPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<Ip2RegionConfig>()
            .expect("ip2region 配置加载失败");

        let ipv4 = Searcher::new(config.ipv4_db_path.clone(), CachePolicy::FullMemory)
            .unwrap_or_else(|e| panic!("加载 IPv4 xdb 失败 {}: {}", config.ipv4_db_path, e));
        tracing::info!("ip2region IPv4 Searcher 已加载: {}", config.ipv4_db_path);

        let ipv6 = config.ipv6_db_path.map(|path| {
            let searcher = Searcher::new(path.clone(), CachePolicy::FullMemory)
                .unwrap_or_else(|e| panic!("加载 IPv6 xdb 失败 {}: {}", path, e));
            tracing::info!("ip2region IPv6 Searcher 已加载: {}", path);
            searcher
        });

        app.add_component(Ip2RegionSearcher(Arc::new(Ip2RegionInner { ipv4, ipv6 })));
    }
}