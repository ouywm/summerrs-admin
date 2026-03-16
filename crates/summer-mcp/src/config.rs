use std::net::{IpAddr, Ipv4Addr};

use serde::Deserialize;
use summer::config::Configurable;

#[derive(Debug, Configurable, Clone, Deserialize)]
#[config_prefix = "mcp"]
pub struct McpConfig {
    /// 是否启用 MCP Server
    #[serde(default)]
    pub enabled: bool,

    /// 传输方式
    #[serde(default)]
    pub transport: McpTransport,

    /// HTTP 模式运行方式
    #[serde(default)]
    pub http_mode: McpHttpMode,

    // ── 服务器元信息（ServerInfo / Implementation） ──
    /// 服务器名称（MCP 协议中上报给客户端）
    #[serde(default = "default_server_name")]
    pub server_name: String,

    /// 服务器版本
    #[serde(default = "default_server_version")]
    pub server_version: String,

    /// 人类可读的服务器标题
    pub title: Option<String>,

    /// 服务器描述
    pub description: Option<String>,

    /// AI Agent 使用说明（告诉 Agent 这个 MCP Server 能做什么）
    pub instructions: Option<String>,

    // ── Streamable HTTP 模式专用 ──
    /// HTTP 模式监听地址
    #[serde(default = "default_binding")]
    pub binding: IpAddr,

    /// HTTP 模式监听端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// MCP 服务路由路径
    #[serde(default = "default_mcp_path")]
    pub path: String,

    /// SSE 心跳间隔（秒），保持连接活跃
    #[serde(default = "default_sse_keep_alive")]
    pub sse_keep_alive: u64,

    /// SSE 重连重试间隔（秒）
    #[serde(default = "default_sse_retry")]
    pub sse_retry: u64,

    /// 是否启用有状态会话模式
    #[serde(default = "default_stateful_mode")]
    pub stateful_mode: bool,

    /// 无状态模式下是否返回 JSON 而非 SSE（减少帧开销）
    #[serde(default)]
    pub json_response: bool,

    // ── 会话配置（SessionConfig） ──
    /// 会话通道缓冲容量
    #[serde(default = "default_session_channel_capacity")]
    pub session_channel_capacity: usize,

    /// 会话不活动超时（秒），None 表示永不超时
    pub session_keep_alive: Option<u64>,

    /// 仅运行时使用，供代码生成类工具复用当前数据库连接串
    #[serde(skip)]
    pub default_database_url: Option<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: McpTransport::default(),
            http_mode: McpHttpMode::default(),
            server_name: default_server_name(),
            server_version: default_server_version(),
            title: None,
            description: None,
            instructions: None,
            binding: default_binding(),
            port: default_port(),
            path: default_mcp_path(),
            sse_keep_alive: default_sse_keep_alive(),
            sse_retry: default_sse_retry(),
            stateful_mode: true,
            json_response: false,
            session_channel_capacity: default_session_channel_capacity(),
            session_keep_alive: None,
            default_database_url: None,
        }
    }
}
/// MCP 传输方式
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// 标准输入输出，被 Claude Code / Cursor 等通过 stdin/stdout 调用
    #[default]
    Stdio,
    /// Streamable HTTP（SSE），独立端口，支持远程 Agent 调用
    Http,
}

/// MCP HTTP 运行方式
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpHttpMode {
    /// 挂载到已有 summer-web 主路由，由主应用统一监听端口
    #[default]
    Embedded,
    /// 独立启动一个 HTTP 监听器
    Standalone,
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        };
        f.write_str(value)
    }
}

impl std::fmt::Display for McpHttpMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Embedded => "embedded",
            Self::Standalone => "standalone",
        };
        f.write_str(value)
    }
}

impl std::str::FromStr for McpTransport {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "stdio" => Ok(Self::Stdio),
            "http" => Ok(Self::Http),
            other => Err(format!("未知传输模式: {other}，可选: stdio, http")),
        }
    }
}

fn default_binding() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}

fn default_port() -> u16 {
    9090
}

fn default_mcp_path() -> String {
    "/mcp".to_string()
}

fn default_server_name() -> String {
    "summerrs-admin-mcp".to_string()
}

fn default_server_version() -> String {
    "0.0.1".to_string()
}

fn default_sse_keep_alive() -> u64 {
    15
}

fn default_sse_retry() -> u64 {
    3
}

fn default_session_channel_capacity() -> usize {
    16
}

fn default_stateful_mode() -> bool {
    true
}
