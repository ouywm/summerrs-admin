# Rust AI 框架全景 (2026-03-16 更新)

> 数据来源：GitHub API 实时查询 + 网络搜索汇总。星星数为截至 2026 年 3 月 16 日的实时数据。

---

## 一、LLM 应用 / Agent 框架（核心关注）

这类框架类似 Python 的 LangChain / LangGraph / CrewAI，用于构建 LLM 驱动的应用和智能体。

| 排名 | 项目 | Stars | Forks | 描述 | 协议 | 链接 |
|:---:|------|------:|------:|------|------|------|
| 1 | **Rig** | 6,519 | 704 | 构建模块化、可扩展的 LLM 应用。统一 LLM 接口，高级 AI 工作流抽象 | MIT | [GitHub](https://github.com/0xPlaygrounds/rig) \| [官网](https://rig.rs) |
| 2 | **llm-chain** | 1,594 | 142 | 类 LangChain 的 Rust 实现。支持 prompt 模板、链式调用、向量存储、摘要 | MIT | [GitHub](https://github.com/sobelio/llm-chain) \| [官网](https://llm-chain.xyz) |
| 3 | **langchain-rust** | 1,254 | 172 | LangChain 的 Rust 移植版。提供 Chain、Agent、向量存储等完整功能 | MIT | [GitHub](https://github.com/Abraxas-365/langchain-rust) |
| 4 | **AutoAgents** | 441 | 61 | 多 Agent 框架。类型安全、结构化工具调用、可配置记忆、可插拔 LLM 后端 | Apache-2.0 | [GitHub](https://github.com/liquidos-ai/AutoAgents) \| [文档](https://liquidos-ai.github.io/AutoAgents/) |
| 5 | **Anda** | 408 | 47 | 基于 ICP 区块链 + TEE 的 AI Agent 框架，Agent 拥有永久身份和加密能力 | Apache-2.0 | [GitHub](https://github.com/ldclabs/anda) \| [官网](https://anda.ai) |
| 6 | **graniet/llm** | 322 | 73 | 统一编排多 LLM/Agent/语音后端（OpenAI, Claude, Gemini, Ollama 等） | MIT | [GitHub](https://github.com/graniet/llm) |
| 7 | **rs-graph-llm** | 262 | 28 | 类 LangGraph 的 Rust 实现。基于图的多 Agent 工作流系统 | MIT | [GitHub](https://github.com/a-agmon/rs-graph-llm) |
| 8 | **ADK-Rust** | 182 | 28 | 生产就绪的 Agent 开发套件。支持 15+ LLM 供应商、实时语音、图工作流 | - | [GitHub](https://github.com/zavora-ai/adk-rust) \| [官网](https://adk-rust.com) |
| 9 | **Swarm** | 8 | 5 | Agent SDK，支持 MCP/A2A 开放标准，可配置文件启动多 Agent 系统 | Apache-2.0 | [GitHub](https://github.com/fcn06/swarm) |

### 各框架特性对比

| 特性 | Rig | llm-chain | langchain-rust | AutoAgents | ADK-Rust |
|------|:---:|:---------:|:--------------:|:----------:|:--------:|
| 多 LLM 供应商 | ✅ | ✅ | ✅ | ✅ | ✅ (15+) |
| 多 Agent 协作 | ✅ | - | ✅ | ✅ | ✅ |
| MCP 支持 | ✅ | - | - | - | ✅ |
| 本地推理 | 通过插件 | ✅ | - | ✅ (mistral.rs) | ✅ (mistral.rs) |
| 向量存储 | ✅ | ✅ | ✅ | ✅ (Qdrant) | - |
| 结构化输出 | ✅ | ✅ | ✅ | ✅ | ✅ |
| 实时语音 | - | - | - | ✅ | ✅ |
| Guardrails | - | - | - | ✅ | - |
| 图工作流 | - | - | - | - | ✅ |
| 活跃度 | 🟢 高 | 🟡 低 | 🟢 中 | 🟢 高 | 🟢 高 |

---

## 二、MCP (Model Context Protocol) Rust 生态

| 项目 | Stars | Forks | 描述 | 链接 |
|------|------:|------:|------|------|
| **rmcp (官方 Rust SDK)** | 3,175 | 478 | Anthropic 官方 MCP Rust SDK，crate 名 `rmcp` | [GitHub](https://github.com/modelcontextprotocol/rust-sdk) |
| **mcp-framework** | - | - | 基于 rmcp 的生产级 AI Agent 框架，内置 MCP 支持 | [GitHub](https://github.com/koki7o/mcp-framework) |
| **mcp-rust-sdk** | - | - | 社区 MCP Rust 实现 | [GitHub](https://github.com/Derek-X-Wang/mcp-rust-sdk) |
| **mcp-protocol-sdk** | - | - | 100% MCP Schema 合规，支持 STDIO/HTTP/WebSocket | [GitHub](https://github.com/mcp-rust/mcp-protocol-sdk) |

---

## 三、LLM 推理引擎（Rust 原生）

这类项目专注于在本地高效运行 LLM 模型。

| 排名 | 项目 | Stars | Forks | 描述 | 链接 |
|:---:|------|------:|------:|------|------|
| 1 | **Candle** | 19,687 | 1,453 | HuggingFace 出品，极简 ML 框架。支持 CPU/CUDA/Metal 推理 | [GitHub](https://github.com/huggingface/candle) |
| 2 | **mistral.rs** | 6,699 | 541 | 快速灵活的 LLM 推理引擎。支持多模态、量化、FlashAttention、PagedAttention | [GitHub](https://github.com/EricLBuehler/mistral.rs) |
| 3 | **Kalosm (Floneum)** | 2,154 | 127 | 本地私有 AI 平台。支持语言/音频/视觉模型，35+ 模型，结构化生成 | [GitHub](https://github.com/floneum/floneum) \| [官网](https://floneum.com/kalosm) |

---

## 四、深度学习框架（Rust 原生）

| 排名 | 项目 | Stars | Forks | 描述 | 链接 |
|:---:|------|------:|------:|------|------|
| 1 | **Burn** | 14,606 | 848 | 全面的动态深度学习框架。支持 CUDA/Metal/Vulkan/WASM/WebGPU | [GitHub](https://github.com/tracel-ai/burn) \| [官网](https://burn.dev) |
| 2 | **Candle** | 19,687 | 1,453 | （同上，也作为深度学习底层框架使用） | [GitHub](https://github.com/huggingface/candle) |

---

## 五、Rust vs Python AI 框架性能基准

根据 2026 年初的公开基准测试数据：

| 指标 | Rust (AutoAgents) | Python (LangGraph) | 差距 |
|------|:-----------------:|:------------------:|:----:|
| 吞吐量 (rps) | 4.97 | 2.70 | Rust 高 84% |
| CPU 使用率 | 24.3% (Rig) | ~60%+ | Rust 低 2.5x |
| 峰值内存 | < 1.1 GB | > 4.7 GB | Rust 低 5x |
| 冷启动时间 | ~4 ms | 60-140 ms | Rust 快 15-35x |
| 内存相关崩溃 | 减少 78% | 基准 | - |

---

## 六、资源汇总列表

| 资源 | 描述 | 链接 |
|------|------|------|
| **awesome-rust-llm** | 精选 Rust LLM 工具/库/框架列表 | [GitHub](https://github.com/jondot/awesome-rust-llm) |
| **best-of-ml-rust** | 230 个 ML 开源项目排名（510K 总 stars） | [GitHub](https://github.com/e-tornike/best-of-ml-rust) |
| **Rust Ecosystem for AI & LLMs** | HackMD 上的 Rust AI 生态系统综述 | [HackMD](https://hackmd.io/@Hamze/Hy5LiRV1gg) |
| **Rust LLM Libraries 2026** | 2026 年 Rust LLM 编排库综述 | [Blog](https://dasroot.net/posts/2026/02/rust-libraries-llm-orchestration-2026/) |
| **GitHub LLM Topic (Rust)** | GitHub 上 Rust 语言的 LLM 话题，477+ 仓库 | [GitHub](https://github.com/topics/llm?l=rust) |

---

## 七、选型建议

### 如果你要构建 LLM 应用/Agent:
- **首选 Rig** — 最成熟、社区最大、文档最全的 Rust LLM 框架
- **需要多 Agent** — 考虑 AutoAgents 或 ADK-Rust
- **熟悉 LangChain** — langchain-rust 提供最相似的 API 体验
- **需要类 LangGraph 图工作流** — rs-graph-llm

### 如果你要本地运行模型:
- **推理引擎首选 mistral.rs** — 性能最佳，功能最全
- **需要底层 ML 能力** — 使用 Candle（HuggingFace 生态）
- **需要语言+音频+视觉全能** — Kalosm

### 如果你要构建 MCP 服务:
- **使用官方 rmcp SDK** — 3K+ stars，官方维护

---

*本文档由 Claude Opus 4.6 于 2026-03-16 通过网络搜索 + GitHub API 自动生成*