use schemars::JsonSchema;
use serde::Deserialize;

fn deserialize_u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// 分页请求参数，接收前端的 current（1起始）和 size
///
/// 前端发送：`?current=1&size=10`
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PageQuery {
    /// 当前页码，从 1 开始（默认 1）
    #[serde(default = "default_current", deserialize_with = "deserialize_u64_from_str")]
    pub current: u64,
    /// 每页条数（默认 10）
    #[serde(default = "default_size", deserialize_with = "deserialize_u64_from_str")]
    pub size: u64,
}

fn default_current() -> u64 {
    1
}

fn default_size() -> u64 {
    10
}

impl PageQuery {
    /// 转换为 0 起始的页码（供 sea-orm paginate 使用）
    pub fn page_index(&self) -> u64 {
        if self.current == 0 { 0 } else { self.current - 1 }
    }
}
