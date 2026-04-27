# ZeroClaw 学习路线（旧版 v1 · 章节简洁版）

> ⚠️ **说明**：此为旧版（v1），采用章节式结构、对比表格、快速导读。更详细的"手把手打卡式"新版请见 [`zeroclaw-learning-path.md`](./zeroclaw-learning-path.md)。两版内容互补，可按需查阅。
>
> **前置**：建议先完成 [IronClaw 学习路线（旧版）](./ironclaw-learning-path.v1.md)
> **目标读者**：有 Rust 基础 + 刚读完 IronClaw
> **作者视角**：Claude Opus 4.7 (1M context)，2026-04-18
> **仓库**：[zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw)
> **预计学习周期**：6~8 周（每天 2~3 小时）

---

## 目录

- [第 0 章：ZeroClaw 的独特性](#第-0-章zeroclaw-的独特性)
- [第 1 章：IronClaw vs ZeroClaw 对比地图](#第-1-章ironclaw-vs-zeroclaw-对比地图)
- [第 2 章：架构总览与心智模型](#第-2-章架构总览与心智模型)
- [第 3 章：环境搭建](#第-3-章环境搭建)
- [第 4 章：Workspace 全景](#第-4-章workspace-全景)
- [第 5 章：zeroclaw-api —— 核心 trait](#第-5-章zeroclaw-api--核心-trait)
- [第 6 章：zeroclaw-config —— 可切换配置](#第-6-章zeroclaw-config--可切换配置)
- [第 7 章：zeroclaw-infra —— 基础设施](#第-7-章zeroclaw-infra--基础设施)
- [第 8 章：zeroclaw-macros —— 过程宏](#第-8-章zeroclaw-macros--过程宏)
- [第 9 章：zeroclaw-tool-call-parser](#第-9-章zeroclaw-tool-call-parser)
- [第 10 章：zeroclaw-providers —— 20+ Provider](#第-10-章zeroclaw-providers--20-provider)
- [第 11 章：zeroclaw-memory —— 多后端记忆](#第-11-章zeroclaw-memory--多后端记忆)
- [第 12 章：zeroclaw-tools —— 内置工具库](#第-12-章zeroclaw-tools--内置工具库)
- [第 13 章：zeroclaw-channels —— 30+ 通道](#第-13-章zeroclaw-channels--30-通道)
- [第 14 章：zeroclaw-runtime —— Agent 心脏](#第-14-章zeroclaw-runtime--agent-心脏)
- [第 15 章：zeroclaw-plugins —— 插件系统](#第-15-章zeroclaw-plugins--插件系统)
- [第 16 章：zeroclaw-gateway —— Webhook 服务](#第-16-章zeroclaw-gateway--webhook-服务)
- [第 17 章：zeroclaw-tui —— 终端界面](#第-17-章zeroclaw-tui--终端界面)
- [第 18 章：zeroclaw-hardware + aardvark-sys + robot-kit](#第-18-章zeroclaw-hardware--aardvark-sys--robot-kit)
- [第 19 章：apps/tauri —— 桌面版](#第-19-章appstauri--桌面版)
- [第 20 章：部署到树莓派](#第-20-章部署到树莓派)
- [第 21 章：实战项目清单](#第-21-章实战项目清单)
- [第 22 章：性能优化手法](#第-22-章性能优化手法)
- [附录 A：调试 & 性能工具](#附录-a调试--性能工具)
- [附录 B：常见坑](#附录-b常见坑)

---

## 第 0 章：ZeroClaw 的独特性

### 0.1 一句话概括

**ZeroClaw = trait-driven 的可替换 Agent 运行时，目标是让同一份架构跑在从 $10 硬件到服务器的任何环境。**

### 0.2 核心设计哲学

| 设计决策 | 原因 |
|---|---|
| 每个子系统都是 trait | 用户配置切换实现，不改代码 |
| 15 个细粒度 crate | 按需 feature gate，嵌入式只带必要的 |
| 自带记忆引擎 | 不依赖 Pinecone/Elasticsearch，可离线 |
| 单二进制，<5MB | 能塞进 embedded flash |
| 支持 RISC-V | 真的跑在 $10 开发板上 |
| 硬件 crate | GPIO/USB/Serial 原生支持 |

### 0.3 学 ZeroClaw 的三个收获

1. **Trait-driven 架构设计**：比 IronClaw 更进阶的工程美学
2. **嵌入式 Rust**：硬件 crate 接触真正的 no_std 思维
3. **极致优化**：理解如何把 AI Agent 压到 MB 级别

### 0.4 ZeroClaw 的"坑"

- **crate 数多（15+）**：一不小心就迷路
- **feature gate 密集**：要梳理
- **硬件部分**门槛高，可选学
- **新版本节奏快**：关注 `CHANGELOG-next.md`

---

## 第 1 章：IronClaw vs ZeroClaw 对比地图

| 概念 | IronClaw | ZeroClaw |
|---|---|---|
| Agent 主循环 | `ironclaw_engine/runtime/` | `zeroclaw-runtime/agent/` |
| 工具抽象 | WASM skill + capability | `zeroclaw-tools/` + trait |
| 记忆后端 | PostgreSQL 单一 | SQLite/Markdown/Qdrant/Vector 多后端 trait |
| Provider | `src/llm/` | `zeroclaw-providers/`（20+） |
| 通道 | 4 个 | 30+（含 IRC/QQ/Nostr/Matrix/iMessage） |
| 沙箱 | WASM（wasmtime） | 路径 allowlist + 命令 allowlist |
| 配置 | .env + onboard | `zeroclaw-config` 可热重载 + 加密 |
| UI | TUI | TUI + Tauri + Web 仪表盘 |
| 硬件 | 无 | `zeroclaw-hardware` + aardvark + robot-kit |
| 安全核心 | `ironclaw_safety` + WASM | pairing + workspace scope + 命令 allowlist + encrypted secrets |

**总结**：IronClaw 是"安全一条路走到黑"，ZeroClaw 是"可替换一切"。

---

## 第 2 章：架构总览与心智模型

### 2.1 trait-driven 架构

```
               ┌──────────────────────────────┐
               │   zeroclaw (main binary)     │
               └──────────────┬───────────────┘
                              │ (feature gate)
        ┌─────────┬───────────┼───────────┬──────────┐
        ▼         ▼           ▼           ▼          ▼
   [runtime]  [tui]     [channels]   [gateway]   [hardware]
        │         │           │           │          │
        └─────────┴─────┬─────┴───────────┴──────────┘
                        ▼
              ┌─────────────────┐
              │  [providers]    │
              │  [memory]       │
              │  [tools]        │
              │  [plugins]      │
              └────────┬────────┘
                       ▼
              ┌──────────────────┐
              │ [api] [config]   │
              │ [infra] [macros] │
              └──────────────────┘
```

### 2.2 消息流

```
User 输入
   ↓
[channels::*] 30+ 实现选一
   ↓
[runtime::security] workspace / 命令 allowlist
   ↓
[runtime::agent] 组 Prompt + 选工具
   ↓
[providers::*] LLM 调用
   ↓
[runtime::tools] 工具执行
   ↓
[memory::*] 多后端写入
   ↓
[channels::*] 回发
```

### 2.3 核心 trait 列表

```
Provider / Memory / Tool / Channel /
Tunnel / Identity / Runtime / Security /
Observability
```

---

## 第 3 章：环境搭建

### 3.1 基础

```bash
rustup default stable
rustc --version  # >= 1.87
```

### 3.2 可选依赖

```bash
# macOS 桌面版
brew install node pnpm
cd apps/tauri && pnpm install

# Qdrant
docker run -p 6333:6333 qdrant/qdrant

# 硬件开发
brew install arm-none-eabi-gcc
```

### 3.3 编译 & 首启

```bash
git clone https://github.com/zeroclaw-labs/zeroclaw.git
cd zeroclaw
just build

./target/release/zeroclaw onboard
./target/release/zeroclaw
```

### 3.4 日志

```bash
RUST_LOG=zeroclaw=debug,zeroclaw_runtime=trace ./target/release/zeroclaw
```

---

## 第 4 章：Workspace 全景

### 4.1 15 个 crate 分层

**Layer 0（契约）**：`zeroclaw-api` / `zeroclaw-macros` / `zeroclaw-config` / `zeroclaw-infra` / `zeroclaw-tool-call-parser`

**Layer 1（能力）**：`zeroclaw-providers` / `zeroclaw-memory` / `zeroclaw-tools` / `zeroclaw-channels` / `zeroclaw-plugins`

**Layer 2（运行时）**：`zeroclaw-runtime` / `zeroclaw-gateway` / `zeroclaw-hardware`

**Layer 3（UI & App）**：`zeroclaw-tui` / `apps/tauri`

**Layer X（硬件驱动）**：`aardvark-sys` / `robot-kit`

### 4.2 依赖图

```
zeroclaw-api ← (所有 crate 都依赖)
    ↑
infra / config / tool-call-parser
    ↑
providers / memory / tools
    ↑
channels / plugins / hardware
    ↑
runtime
    ↑
tui / gateway
    ↑
zeroclaw (main) + apps/tauri
```

**阅读顺序即按此图**。

---

## 第 5 章：zeroclaw-api —— 核心 trait

**整个项目的"宪法"**。必须第一个读透。

### 5.1 预期内容

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
}

#[async_trait]
pub trait Memory: Send + Sync {
    async fn store(&self, item: MemoryItem) -> Result<()>;
    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemoryItem>>;
}

#[async_trait]
pub trait Channel: Send + Sync {
    async fn recv(&mut self) -> Option<IncomingMessage>;
    async fn send(&self, msg: OutgoingMessage) -> Result<()>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn call(&self, args: Value) -> Result<Value>;
}
```

### 5.2 阅读方法

1. `cargo doc -p zeroclaw-api --open`
2. 列出所有 `pub trait`
3. 每个写一句话：**输入什么、输出什么、谁实现、谁调用**
4. 用 Mermaid 画 trait 关系图

### 5.3 产出

`api-contract-map.md`，记录所有 trait 签名 + 一句话作用。

---

## 第 6 章：zeroclaw-config —— 可切换配置

### 6.1 为什么独立 crate

trait 选择由配置驱动，配置解析必须在所有能力 crate 之前就绪。

### 6.2 预期特性

- TOML 解析
- 热重载（watch 文件）
- 加密 secrets（local key 解密）
- 多环境（dev/prod）

### 6.3 练习

给 `~/.zeroclaw/config.toml` 加一行 `[providers.anthropic] api_key = "..."`，观察生效。

---

## 第 7 章：zeroclaw-infra —— 基础设施

官方描述：**debounce / session / stall watchdog**

### 7.1 关键组件

- **Debounce**：连发消息合并处理
- **Session**：跨消息上下文保持
- **Stall Watchdog**：LLM 卡死自动中断

### 7.2 要理解

- `tokio::select!` 的多路等待
- 定时器与取消信号

---

## 第 8 章：zeroclaw-macros —— 过程宏

### 8.1 可能的宏

- `#[tool]` —— 函数变 Tool trait 实现
- `#[derive(Provider)]` —— 自动实现
- `#[memory_backend]` —— 注册到全局表

### 8.2 学习要点

- `syn` / `quote` 基础
- `cargo expand` 看展开

### 8.3 练习

读一个宏 → `cargo expand` 展开 → 对照手写版。

---

## 第 9 章：zeroclaw-tool-call-parser

### 9.1 背景

LLM 工具调用格式多样：OpenAI tool_calls / Anthropic tool_use / Gemini functionCall / XML / JSON markdown。

这个 crate 把所有格式解析成**统一 IR**。

### 9.2 要读

- 各 provider fixture
- 解析状态机
- 错误恢复

### 9.3 练习

给新模型（假设 Kimi）加解析器：5 样本 → 写 parser → 单测。

---

## 第 10 章：zeroclaw-providers —— 20+ Provider

### 10.1 阅读顺序

1. `traits.rs` —— 契约
2. `openai.rs` —— 最经典
3. `anthropic.rs` —— 有 Reasoning/Thinking
4. `ollama.rs` —— 本地
5. `compatible.rs` —— OpenAI 兼容网关
6. `reliable.rs` —— 重试/降级 wrapper
7. `router.rs` —— 多 Provider 路由

### 10.2 重点：装饰器栈

```
OpenAI 实例
   ↓ 包裹
Reliable(OpenAI)  // 加重试
   ↓ 包裹
Router([Reliable(OpenAI), Reliable(Anthropic)])
```

### 10.3 练习

实现 Kimi 或 Doubao Provider。

---

## 第 11 章：zeroclaw-memory —— 多后端记忆

### 11.1 概念地图

- **存储后端**：markdown / sqlite / qdrant / none
- **检索层**：retrieval / vector
- **向量化**：embeddings / chunker
- **卫生层**：hygiene / conflict / consolidation / decay
- **重要性**：importance
- **知识图谱**：knowledge_graph
- **审计/快照**：audit / snapshot
- **命名空间**：namespaced / policy
- **缓存**：response_cache

### 11.2 亮点：decay & consolidation

类似人脑：
- **decay**：久不访问衰减
- **consolidation**：相关记忆合并成摘要

### 11.3 练习

1. SQLite 切 Markdown 后端
2. 调 decay 参数观察召回
3. 实现新后端（redis-vector 或 libsql）

---

## 第 12 章：zeroclaw-tools —— 内置工具库

**60+ 文件**，按类别抽样读。

### 12.1 分类

| 类别 | 示例 |
|---|---|
| 文件系统 | file_edit / glob_search / content_search |
| 网络 | http_request / web_fetch / web_search_tool |
| Git | git_operations |
| 浏览器 | browser / screenshot / text_browser |
| AI 嵌套 | claude_code_runner / codex_cli / gemini_cli |
| Memory | memory_recall / memory_store / memory_forget |
| 外部服务 | notion / jira / linkedin / composio / microsoft365 |
| 硬件 | hardware_board_info / hardware_memory_map |
| MCP | mcp_client / mcp_protocol / mcp_transport / mcp_tool |

### 12.2 推荐阅读

1. `calculator.rs` —— 最简 Tool 模板
2. `http_request.rs` —— 完整外部调用
3. MCP 4 件套 —— 理解 MCP 接入
4. `claude_code_runner.rs` —— Agent 套 Agent 模式

### 12.3 练习

实现 `bilibili_search` 工具，通过 MCP 暴露。

---

## 第 13 章：zeroclaw-channels —— 30+ 通道

### 13.1 通道协议分类

| 协议 | 代表 |
|---|---|
| Webhook | telegram / slack / dingtalk / lark / wecom |
| 长连接 Gateway | discord / matrix / irc / nostr |
| 浏览器桥接 | whatsapp_web |
| 系统 API | imessage（macOS） |
| 邮件 | email_channel / gmail_push |
| 语音 | voice_call / voice_wake / transcription / tts |
| 中国场景 | dingtalk / lark / wecom / mochat / qq |

### 13.2 推荐阅读

1. `cli.rs` —— 开胃菜
2. `telegram.rs` —— webhook 经典
3. `discord.rs` —— 长连接 gateway
4. `matrix.rs` —— 去中心化
5. 语音闭环：voice_wake + transcription + tts
6. `whatsapp_web.rs` —— 最硬核
7. `orchestrator/` —— 多通道编排

### 13.3 练习

实现飞书机器人 v2 或 Keybase。

---

## 第 14 章：zeroclaw-runtime —— Agent 心脏

最大的 crate。

### 14.1 阅读路线

**第 1 周：主循环**
1. `lib.rs` 看导出
2. `agent/` 主循环
3. `sop/` Standard Operating Procedures
4. `routines/` 周期任务
5. `cron/` 定时任务

**第 2 周：安全与信任**
6. `security/` / `trust/` / `approval/` / `verifiable_intent/`

**第 3 周：运维**
10. `health/` + `doctor/`
11. `observability/` / `cost/`
13. `tunnel/`

**第 4 周：进阶**
14. `skills/` + `skillforge/`
15. `rag/` / `hooks/` / `integrations/` / `nodes/`

### 14.2 SOP 系统

SOP = 预定义操作流程。比纯 LLM 自由发挥更可控、更省 token。

### 14.3 verifiable_intent

动手前先输出**可人类审计的计划**，用户确认后才执行。

### 14.4 练习

1. 写自定义 SOP："每天早上发送昨日总结"
2. 挂 hook，每次工具调用前打日志
3. 调 cost 追踪看对话成本

---

## 第 15 章：zeroclaw-plugins —— 插件系统

### 15.1 对比 IronClaw

| | IronClaw | ZeroClaw |
|---|---|---|
| 扩展机制 | WASM skill | plugins crate + MCP |
| 隔离 | WASM 沙箱 | 进程 / trait 注入 |

### 15.2 学习要点

- 插件发现机制
- 元数据格式
- 加载/卸载

---

## 第 16 章：zeroclaw-gateway —— Webhook 服务

内嵌 HTTP 服务器，接收 Telegram/Slack 等 webhook。

- `build.rs` 看资源嵌入
- 读路由
- 看签名校验

---

## 第 17 章：zeroclaw-tui —— 终端界面

对照 IronClaw TUI，大体相似（都是 ratatui）。

重点看**差异点**：多标签页、命令板等。

---

## 第 18 章：zeroclaw-hardware + aardvark-sys + robot-kit

### 18.1 门槛提示

**可选**。不玩嵌入式快速扫过。

### 18.2 zeroclaw-hardware

- USB 设备发现
- Serial 串口
- GPIO 控制
- 板级信息

### 18.3 aardvark-sys

对 Total Phase 公司 [Aardvark I²C/SPI Host Adapter](https://www.totalphase.com/products/aardvark-i2cspi/) 的 FFI 绑定。`vendor/` 是官方 SDK。

### 18.4 robot-kit

机器人控制抽象。看 `PI5_SETUP.md` 和 `SOUL.md`。

### 18.5 练习（有树莓派）

1. 跑 PI5_SETUP 最小示例
2. AI 点亮 LED
3. 加传感器工具（温度计）

---

## 第 19 章：apps/tauri —— 桌面版

### 19.1 Tauri 概览

Rust 后端 + Web 前端，比 Electron 轻 10 倍。

### 19.2 学习要点

- Rust ↔ JS 通信（invoke / emit）
- 窗口管理
- 与 runtime crate 集成

---

## 第 20 章：部署到树莓派

### 20.1 交叉编译

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu --no-default-features
```

### 20.2 最小 feature

```toml
[features]
minimal = ["channels/cli", "providers/openai", "memory/sqlite"]
```

### 20.3 内存调优

- 关闭未用 feature
- `[profile.release] lto = "fat"`
- `strip = "symbols"`
- `panic = "abort"`

---

## 第 21 章：实战项目清单

1. **热身**：默认 provider 从 OpenAI 切 Anthropic
2. **入门**：`#[tool]` 宏写汇率查询
3. **进阶**：实现新 memory 后端（libsql / redis-vector）
4. **挑战**：实现新通道（飞书 / Keybase / Revolt）
5. **硬核**：给树莓派写 LED 控制工具
6. **架构级**：SOP 执行引擎重写为状态机
7. **极限**：二进制压到 3MB 以内

---

## 第 22 章：性能优化手法

### 22.1 二进制体积

```bash
cargo bloat --release --crates | head -30
cargo bloat --release | head -30
```

常见优化：
- `regex` → `regex-lite`
- `serde_json` → `serde-json-core`（no_std）
- `reqwest` → `ureq` 或 `hyper` 直接用

### 22.2 启动时间

```bash
samply record ./target/release/zeroclaw
```

重点看：
- 配置加载
- Provider 初始化（懒加载）
- DB 连接（延迟到首次使用）

### 22.3 内存 footprint

- 避免 Clone Arc<T>
- 避免 Vec 预分配过大
- 选对 async runtime（current_thread > multi_thread）

---

## 附录 A：调试 & 性能工具

```bash
# 子 crate 编译
cargo build -p zeroclaw-runtime

# 只测一个 crate
cargo test -p zeroclaw-memory

# 依赖树
cargo tree -p zeroclaw-providers

# 展开宏
cargo install cargo-expand
cargo expand -p zeroclaw-macros

# 二进制分析
cargo install cargo-bloat
cargo bloat --release

# Profile
samply record ./target/release/zeroclaw
```

---

## 附录 B：常见坑

**Q1：编译慢？**
15 个 crate + Tauri，首编半小时。日常用 `cargo check`。

**Q2：找不到 trait 实现？**
`rg "impl .* for YourType"` 全局搜。

**Q3：feature 冲突？**
`cargo tree -e features` 看谁打开哪个 feature。

**Q4：Tauri 打不开？**
`cd apps/tauri && pnpm install && pnpm tauri dev`

**Q5：硬件 crate 编译失败？**
默认 feature 不含硬件。`--features hardware` 才编。

**Q6：为什么这么多 Provider？**
trait-driven，新增基本复制 `openai.rs` 改 URL，数量膨胀快。

---

## 完成标志（学完自测）

- [ ] 能口头讲清 15 crate 职责边界
- [ ] 能写 `zeroclaw-api` 至少 6 个核心 trait 签名
- [ ] 会用 `#[tool]` 宏写工具
- [ ] 能替换 Provider / Memory / Channel 实现
- [ ] 能用 feature gate 构建 <3MB 嵌入式二进制
- [ ] 理解 decay/consolidation 对 memory 的影响
- [ ] 能解释 SOP / verifiable_intent / approval 三者关系
- [ ] 读过至少 1 个硬件模块

全部打勾 → 你可以自信说"我懂 Rust AI Agent 架构"。

---

## 下一步

1. **深入一层**：挑 Provider / Memory / Tool / Channel 最感兴趣的读完所有实现
2. **造轮子**：用 `genai` + `Rig` 写精简版 Claw（5k~10k 行）
3. **回馈社区**：给 IronClaw / ZeroClaw 提 PR
4. **研究方向**：MCP 生态 / Agent 评测 / 本地小模型 / Agent 安全