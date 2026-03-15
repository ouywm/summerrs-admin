# MCP Crate 搭建规划

## 目标

在项目中创建独立的 `crates/mcp` crate，作为 Summer Plugin 无缝集成到现有应用。
提供 MCP 协议接入能力，让 AI Agent 通过 Function Call 调用项目内的工具方法。

**本阶段只搭建 crate 骨架，不实现具体业务 Tools。**

---

## 目录结构

```
crates/mcp/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块导出 + re-export
    ├── config.rs           # McpConfig（TOML 配置映射）
    ├── plugin.rs           # McpPlugin（Summer Plugin trait 实现）
    └── server.rs           # AdminMcpServer（MCP Server 主体骨架）
```

---

## 集成方式

### 1. Workspace 注册

根 `Cargo.toml`：
- `[workspace].members` 新增 `"crates/mcp"`
- `[workspace.dependencies]` 新增 `rmcp` 和 `mcp` 路径依赖

### 2. App 依赖

`crates/app/Cargo.toml`：
- 新增 `mcp = { workspace = true }`

### 3. 主应用接入

`crates/app/src/main.rs`：
```rust
.add_plugin(McpPlugin)  // 一行接入
```

---

## 配置

```toml
# config/app-dev.toml
[mcp]
enabled = true
transport = "stdio"       # "stdio" | "http"
# http 模式专用
host = "127.0.0.1"
port = 9090
```

- `enabled = false` 时 Plugin 直接跳过，零开销
- `stdio` 模式：后台 spawn，被 Claude Code / Cursor 等通过 stdin/stdout 调用
- `http` 模式：Streamable HTTP，独立端口，支持远程 Agent 调用

---

## Plugin 生命周期

```
App::new()
    .add_plugin(SeaOrmPlugin)       // ① 数据库连接就绪
    .add_plugin(RedisPlugin)        // ② Redis 就绪
    .add_plugin(McpPlugin)          // ③ MCP 从容器获取 DbConn 等组件
    .run()

McpPlugin::build():
    1. 读取 [mcp] 配置
    2. 若 enabled = false → return
    3. 从组件容器获取 DbConn（等已有组件）
    4. 创建 AdminMcpServer（注入依赖）
    5. 根据 transport 启动：
       - stdio → tokio::spawn 后台运行
       - http  → 启动独立 Axum listener
```

dependencies: `["sea-orm"]` — 确保数据库插件先于 MCP 插件构建。

---

## Server 骨架

```rust
#[derive(Clone)]
pub struct AdminMcpServer {
    db: DbConn,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AdminMcpServer {
    pub fn new(db: DbConn) -> Self {
        Self {
            db,
            tool_router: Self::tool_router(),
        }
    }

    // Phase 2+ 在这里追加 #[tool] 方法
    // #[tool(description = "...")]
    // async fn my_tool(&self, ...) -> Result<CallToolResult, McpError> { }
}

#[tool_handler]
impl ServerHandler for AdminMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "summerrs-admin-mcp".into(),
                version: "0.0.1".into(),
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}
```

后续开发 Tool 时，只需在 `#[tool_router] impl` 块里加 `#[tool]` 方法即可，框架全部就绪。

---

## Cargo 依赖

### crates/mcp/Cargo.toml

```toml
[package]
name = "mcp"
version = "0.0.1"
edition = "2024"

[dependencies]
rmcp = { workspace = true, features = [
    "server",
    "macros",
    "transport-io",
    "transport-streamable-http-server",
    "schemars",
] }
model = { workspace = true }
common = { workspace = true }
summer = { workspace = true }
sea-orm = { workspace = true, features = ["with-chrono", "sqlx-postgres", "runtime-tokio-rustls"] }
tokio = { workspace = true, features = ["full"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
schemars = { workspace = true, features = ["derive"] }
tracing = { workspace = true }
```

### 根 Cargo.toml 新增

```toml
[workspace.members]  # 新增
"crates/mcp"

[workspace.dependencies]  # 新增
rmcp = { path = "/Volumes/990pro/code/rust/rust-mcp/crates/rmcp" }
mcp = { path = "crates/mcp" }
```

---

## 实现检查清单

- [ ] 创建 `crates/mcp/` 目录和 `Cargo.toml`
- [ ] 根 `Cargo.toml` workspace members + dependencies 注册
- [ ] `src/config.rs` — McpConfig 结构体 + Configurable derive
- [ ] `src/plugin.rs` — McpPlugin 实现 Plugin trait
- [ ] `src/server.rs` — AdminMcpServer 空骨架（tool_router + ServerHandler）
- [ ] `src/lib.rs` — 模块导出
- [ ] `crates/app/Cargo.toml` 加 `mcp` 依赖
- [ ] `crates/app/src/main.rs` 加 `.add_plugin(McpPlugin)`
- [ ] `config/app-dev.toml` 加 `[mcp]` 配置段
- [ ] `cargo check` 通过