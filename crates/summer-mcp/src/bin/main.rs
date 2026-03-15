//! summerrs-mcp -- 独立运行的 MCP Server
//!
//! 用法:
//!   cargo install --path crates/summer-mcp --features standalone
//!   summerrs-mcp --database-url postgres://user:pass@localhost/db
//!
//! 或直接运行:
//!   cargo run -p summer-mcp --features standalone --bin summerrs-mcp -- --database-url ...

use clap::Parser;
use sea_orm::{ConnectOptions, Database};
use summer_mcp::{
    config::{McpConfig, McpHttpMode, McpTransport},
    run_server_with_shutdown,
};

/// summerrs-admin MCP Server (standalone)
#[derive(Parser)]
#[command(name = "summerrs-mcp", version, about)]
struct Cli {
    /// 数据库连接 URL (PostgreSQL)
    #[arg(short, long, env = "DATABASE_URL")]
    database_url: String,

    /// 传输模式
    #[arg(short, long, default_value = "stdio")]
    transport: McpTransport,

    /// HTTP 监听地址 (仅 http 模式)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// HTTP 监听端口 (仅 http 模式)
    #[arg(short, long, default_value_t = 9090)]
    port: u16,

    /// MCP 端点路径 (仅 http 模式)
    #[arg(long, default_value = "/mcp")]
    path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 日志初始化: RUST_LOG=info (默认)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // 连接数据库
    tracing::info!("connecting to database...");
    let opt = ConnectOptions::new(&cli.database_url);
    let db = Database::connect(opt).await?;
    tracing::info!("database connected");

    // 构建 MCP 配置
    let mut config = McpConfig::default();
    config.transport = cli.transport;
    config.http_mode = McpHttpMode::Standalone;
    config.port = cli.port;
    config.binding = cli.host.parse()?;
    config.path = cli.path.clone();
    config.default_database_url = Some(cli.database_url.clone());

    let run_result = run_server_with_shutdown(config, db.clone(), async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await;

    tracing::info!("closing standalone database connection...");
    match db.close().await {
        Ok(()) => tracing::info!("standalone database connection closed"),
        Err(error) => {
            if run_result.is_ok() {
                return Err(anyhow::Error::from(error));
            }
            tracing::warn!("failed to close standalone database connection: {error}");
        }
    }

    run_result.map_err(anyhow::Error::from_boxed)
}
