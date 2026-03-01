use spring_web::axum::http::HeaderMap;
use woothee::parser::Parser;

/// User-Agent 解析结果
#[derive(Debug, Clone)]
pub struct UserAgentInfo {
    pub raw: String,
    pub browser: String,
    pub browser_version: String,
    pub os: String,
    pub os_version: String,
    pub device: String,
}

impl UserAgentInfo {
    /// 从 HeaderMap 提取并解析 User-Agent
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let raw = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown")
            .to_string();

        Self::parse(raw)
    }

    /// 从 User-Agent 字符串解析
    pub fn parse(user_agent: impl Into<String>) -> Self {
        let raw = user_agent.into();
        let parser = Parser::new();

        let (browser, browser_version, os, os_version, device) = if let Some(result) = parser.parse(&raw) {
            (
                result.name.to_string(),
                result.version.to_string(),
                result.os.to_string(),
                result.os_version.to_string(),
                result.category.to_string(),
            )
        } else {
            (
                "Unknown".to_string(),
                String::new(),
                "Unknown".to_string(),
                String::new(),
                "Unknown".to_string(),
            )
        };

        Self {
            raw,
            browser,
            browser_version,
            os,
            os_version,
            device,
        }
    }
}
