# Rust Claw 生态 & AI Agent/Provider 框架调研

> 整理时间：2026-04-18
> 模型：Claude Opus 4.7 (1M context)
> 来源：WebSearch 多轮聚合（见文末链接）

---

## 一、背景：OpenClaw 家族的崛起

**OpenClaw**（原 TypeScript/Node 实现）在 2026 年初爆火（150K+ stars），它把 AI Agent 做成了「多通道 + 长期记忆 + 工具调用」的个人助手。但 Node 运行时带来较大内存和启动开销，于是大量 Rust 重写版本涌现，形成了 **Claw 家族**。

核心痛点：
- **安全** —— 加密凭证 / 沙箱 / 防提示注入
- **性能** —— 单个小二进制 / MB 级内存 / 毫秒启动
- **可移植** —— 树莓派 / ARM / RISC-V / 低配 VPS
- **Provider 无关** —— Anthropic / OpenAI / Ollama / 本地模型自由切换

---

## 二、Rust 实现的 Claw 项目全景

### 1. IronClaw（nearai/ironclaw）⭐ 推荐入门
- **定位**：OpenClaw 启发的 Rust 重写，安全与隐私优先
- **特点**：
  - WASM 沙箱跑不受信任工具，能力式权限（HTTP/secrets/tool 调用需显式 opt-in）
  - 端点 allowlist、凭证注入在 host 边界、泄漏检测
  - 持久记忆 = FTS + 向量 RRF 混合检索（PostgreSQL + pgvector）
  - 身份文件：`AGENTS.md` / `SOUL.md` / `USER.md` / `IDENTITY.md` 注入 system prompt
  - Heartbeat 系统：每 30 分钟主动执行一次
  - Trusted skills vs Installed skills 的信任模型
- **Provider**：默认 NEAR AI；支持 Anthropic / OpenAI / Copilot / Gemini / MiniMax / Mistral / Ollama
- **架构**：6 个 crate（common / engine / gateway / safety / skills / tui）
- **GitHub**：[nearai/ironclaw](https://github.com/nearai/ironclaw)

### 2. ZeroClaw（zeroclaw-labs/zeroclaw）⭐ 性能极致
- **定位**：100% Rust，零开销，$10 硬件 + <5MB 内存
- **特点**：
  - ~3.4MB 二进制，<10ms 启动，1017 个测试
  - **trait-driven**：providers / channels / tools / memory / tunnels / runtime / security / identity / observability 都是 trait，通过配置切换实现
  - 自建记忆引擎（向量 + 关键字），无需 Pinecone / Elasticsearch
  - 23+ Provider；多通道：Telegram/Discord/Slack/WhatsApp/Signal/iMessage/Matrix/IRC/Lark/DingTalk/QQ/Nostr/Email
  - 硬件支持：aardvark-sys、robot-kit（USB/Serial/GPIO）
  - 跨 ARM / x86 / RISC-V 单一二进制
- **架构**：15 个 crate（细粒度）
- **GitHub**：[zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw)

### 3. OpenCrust（opencrust-org/opencrust）
- **定位**：OpenClaw 的 Rust 改写，强调单二进制 + 加密凭证
- **特点**：
  - 单个 16MB 二进制，空闲 13MB RAM
  - AES-256-GCM 凭证加密，配置热重载
  - 多通道：Telegram/Discord/Slack/WhatsApp/WhatsApp Web/LINE/WeChat/iMessage/MQTT
  - 提供 `opencrust migrate openclaw` 一键迁移命令
- **GitHub**：[opencrust-org/opencrust](https://github.com/opencrust-org/opencrust)

### 4. openclaw-rs（neul-labs/openclaw-rs）
- **定位**：Neul Labs 的独立 Rust 实现
- **特点**：Provider 集成使用官方公开 API，独立实现，无 OpenClaw 代码继承
- **GitHub**：[neul-labs/openclaw-rs](https://github.com/neul-labs/openclaw-rs)

### 5. MicroClaw
- **定位**：channel-agnostic 的 agent runtime，专注"聊天中的助手"
- **特点**：单二进制 + SQLite 持久化 + MCP 外部技能联邦 + 可恢复会话

### 6. ZeptoClaw / FemtoClaw
- 极简方向：4MB 级别，主打安全/最小化（搜索结果中 awesome-claw 列表提到，未找到独立官方仓库）

### 7. Claw Code（claw-code.codes）& ClawCR
- **定位**：Claude Code 架构的 clean-room 重写（AI 编程助手，而非个人助手）
- **特点**：
  - Rust ~4000 行 + Python ~1500 行元数据层
  - 6 个核心工具：Bash / Read / Write / Edit / Glob / Grep
  - Provider agnostic：Claude / OpenAI / z.ai / Qwen / Deepseek / 本地
- **GitHub**：[7df-lab/claw-code-rust](https://github.com/7df-lab/claw-code-rust)

### 8. 生态汇总
- [qhkm/awesome-claw](https://github.com/qhkm/awesome-claw) —— 策展的 Claw 家族列表

---

## 三、Rust 的 AI Agent / Provider 框架（底层库）

Claw 家族用的都是下面这些"轮子"。想自己造 Claw，必须先吃透这一层。

### A. 多 Provider 统一接口

| 库 | 地位 | 亮点 |
|---|---|---|
| **[genai](https://github.com/jeremychone/rust-genai)** | 最活跃、最成熟 | OpenAI/Anthropic/Gemini/Ollama/Groq/xAI/DeepSeek/Cohere 全覆盖，归一化 Chat API，原生 Gemini/Anthropic 协议（含 Reasoning） |
| **[llm crate](https://docs.rs/llm)** | 通用接口 + Agent | 统一 API + Agent 模块 + 指数退避重试 + Evaluator 对比评测 |
| **[edgequake-llm](https://github.com/raphaelmansuy/edgequake-llm)** | 企业级 | 加 OpenAI/Bedrock/Azure，含缓存/限流/成本追踪/tracing |
| **turbine-llm / multi-llm / ai_client** | 轻量替代 | 切换接口为主 |

### B. Agent 框架

| 库 | 定位 | 特点 |
|---|---|---|
| **[Rig](https://github.com/0xPlaygrounds/rig)** ⭐ | 生态最大 | 0xPlaygrounds，被 Coral Protocol/VT Code/Neon/Listen 采用；统一 Provider + 向量库（MongoDB/SQLite/内存） |
| **[Swiftide](https://swiftide.rs/)** | 流式 + RAG + Agent | `#[tool]` 宏、生命周期 hooks、Tera 模板，AgentContext 共享状态 |
| **[AutoAgents](https://github.com/liquidos-ai/AutoAgents)** | 多 Agent 协同 | 基于 Ractor actor；云/边/WASM 全平台；guardrails + mistral-rs/llama.cpp 后端 + Qdrant |
| **[rs-graph-llm](https://github.com/a-agmon/rs-graph-llm)** | 图工作流 | 基于 Rig，支持运行时分支 |
| **[agentai](https://docs.rs/agentai)** | 轻量 | 基于 genai 的 ToolBox |
| **[ai-agents](https://crates.io/crates/ai-agents)** | 声明式 | 单个 YAML 定义 agent，多 provider 自动 fallback |

### C. 本地 LLM 推理

| 库 | 用途 | 备注 |
|---|---|---|
| **[Kalosm](https://github.com/floneum/floneum)** ⭐ | 高层 API | 文本/音频/视觉/结构化生成；基于 Candle；CUDA/Metal；Fusor (WebGPU) 规划中 |
| **[mistral.rs](https://github.com/EricLBuehler/mistral.rs)** | 高性能 | 量化 + Apple Silicon / CUDA |
| **candle / candle-transformers** | 底层 | HuggingFace 风格推理 |
| **llama_cpp / llm_client** | C++ 绑定 | 安全封装 llama.cpp |
| rustformers/llm | ❌ 已存档 | 迁移到上面的继任者 |

---

## 四、推荐学习路径

核心思路：**从"用"到"造"，从简单到复杂，从 Agent 框架到完整 Claw**。

### 🎯 路径总览（5 个阶段）

```
阶段 0：基础准备（Rust 异步 / trait / tokio）
          ↓
阶段 1：Provider 层（genai）—— 理解"怎么调 LLM"
          ↓
阶段 2：Agent 框架（Rig 或 Swiftide）—— 理解"怎么组装 Agent"
          ↓
阶段 3：入门 Claw —— IronClaw（架构清晰，6 crate）
          ↓
阶段 4：进阶 Claw —— ZeroClaw（trait 驱动，15 crate，含硬件）
          ↓
阶段 5：造轮子 —— 用 Rig + genai 写自己的迷你 Claw
```

### 📅 详细计划

**阶段 0：前置（1 周）**
- 熟悉 `tokio` / `async-trait` / `serde` / `reqwest`
- 读 IronClaw 的 `Cargo.toml` workspace 配置，理解 Rust 多 crate 组织

**阶段 1：Provider 层（1 周）** —— 入口
- 📖 [genai README](https://github.com/jeremychone/rust-genai) 跑通 OpenAI / Anthropic / Ollama 三个示例
- 对比自己本地 `summerrs-admin/crates/summer-ai/` 的 Provider 抽象
- 目标：能独立封装一个支持流式 + 结构化输出的 Client

**阶段 2：Agent 框架（2 周）** —— 组合
- 主攻 **Rig**（生态最大）：先跑官方 examples，再看 `rig::providers` 模块
- 辅修 **Swiftide**：理解 `#[tool]` 宏 + 生命周期 hooks
- 目标：用 Rig 实现一个带工具调用 + 向量检索的小 Agent

**阶段 3：IronClaw（2 周）** —— 第一个真实 Claw
- 按 `common → safety → engine → skills → gateway → tui` 顺序读
- 重点：**WASM 沙箱实现、凭证边界注入、提示注入防御**
- 跑通本地 REPL，实现一个自定义 skill
- 目标：理解"安全优先"的 Agent 架构该长什么样

**阶段 4：ZeroClaw（3 周）** —— 进阶架构
- 读 trait 定义：`runtime / providers / channels / memory / tunnels`
- 对比 IronClaw：**为什么 ZeroClaw 要拆 15 个 crate？**
- 重点模块：
  - `zeroclaw-runtime`（agent 循环 + cron + SOP）
  - `zeroclaw-memory`（markdown/sqlite/embeddings/vector merge）
  - `zeroclaw-hardware`（USB/GPIO —— 可选）
- 目标：理解 trait-driven 可替换架构，独立替换一个实现（如把 SQLite 换 PostgreSQL）

**阶段 5：造轮子（持续）**
- 用 `genai + Rig + sqlx + axum` 写自己的 mini-Claw
- 选配：加 WASM 沙箱（参考 IronClaw）、加多通道（参考 ZeroClaw）
- 将心得贡献回 ironclaw / zeroclaw

### ⚠️ 不建议的顺序
- ❌ 先学 ZeroClaw：15 crate + 硬件 + trait 抽象，认知负担过重
- ❌ 先学 Claw Code：它是"编程助手"方向，和个人 Agent 体系差异大
- ❌ 跳过 Provider 层直接读 Agent：会被流式/工具调用/结构化输出的协议细节卡住

---

## 五、快速决策表

| 你的目标 | 推荐项目 |
|---|---|
| 学 Agent 架构入门 | IronClaw |
| 追求极致性能 / 嵌入式 | ZeroClaw |
| 做 AI 编程助手 | Claw Code / ClawCR |
| 只想调多个 LLM | genai |
| 做 RAG + Agent | Rig 或 Swiftide |
| 本地离线跑模型 | Kalosm |
| 多 Agent 协同 | AutoAgents |

---

## Sources

- [nearai/ironclaw](https://github.com/nearai/ironclaw)
- [zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw)
- [opencrust-org/opencrust](https://github.com/opencrust-org/opencrust)
- [neul-labs/openclaw-rs](https://github.com/neul-labs/openclaw-rs)
- [openclaw/openclaw](https://github.com/openclaw/openclaw)
- [qhkm/awesome-claw](https://github.com/qhkm/awesome-claw)
- [7df-lab/claw-code-rust](https://github.com/7df-lab/claw-code-rust)
- [Claw Code 官网](https://claw-code.codes/)
- [jeremychone/rust-genai](https://github.com/jeremychone/rust-genai)
- [0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig) / [rig.rs](https://www.rig.rs/)
- [swiftide.rs](https://swiftide.rs/)
- [liquidos-ai/AutoAgents](https://github.com/liquidos-ai/AutoAgents)
- [a-agmon/rs-graph-llm](https://github.com/a-agmon/rs-graph-llm)
- [agentai docs](https://docs.rs/agentai)
- [ai-agents crate](https://crates.io/crates/ai-agents)
- [floneum/floneum (Kalosm)](https://github.com/floneum/floneum)
- [edgequake-llm](https://github.com/raphaelmansuy/edgequake-llm)
- [llm crate](https://docs.rs/llm)
- [Rust Ecosystem for AI & LLMs - HackMD](https://hackmd.io/@Hamze/Hy5LiRV1gg)
- [awesome-rust-llm](https://github.com/jondot/awesome-rust-llm)