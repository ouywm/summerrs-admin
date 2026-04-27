# Rust AI / LLM 库调研（2026-04-16）

> 本文档是面向当前 `summerrs-admin` 仓库的定制化调研，不替代已有的 `docs/rust-ai-frameworks.md`。  
> 本次重点不是做“全网最全榜单”，而是回答一个更实际的问题：**在这个项目里，AI 能力层应该怎么选型、怎么分层、哪些库值得继续跟进。**

## 1. 调研范围

本次调研按下面几类分组：

1. LLM 应用 / Agent 框架
2. 本地推理 / Transformers / 模型运行时
3. 厂商 SDK / OpenAI 兼容 API 客户端
4. MCP 生态
5. AI Gateway / Proxy / Router
6. 向量库 / Embedding / RAG 适配层

调研方法：

- 优先看官方 GitHub 仓库、官方文档、`docs.rs`、`crates.io`
- 尽量避免只看二手博客结论
- 以“项目定位、维护状态、能力边界、适配当前仓库”作为主要判断标准

---

## 2. 当前仓库现状

从仓库代码看，当前已经明确接入或依赖了这些方向：

- `rig-core = "0.33.0"`，并且已经有 `crates/summer-rig`
- `rmcp = "1.2.0"`，并且已有 `summer-mcp`
- 业务结构是 `summer` 插件式架构，适合把 AI 能力做成独立插件/组件，而不是把业务直接绑到某家厂商 SDK

这意味着本项目真正需要的不是“再找一个能调 OpenAI 的库”，而是：

- 一个上层 AI 应用抽象层
- 一个可选的本地推理层
- 一个稳定的 MCP 层
- 一个可替换的向量/RAG 适配层

---

## 3. 结论摘要

### 3.1 推荐组合

对于这个项目，我的推荐组合仍然是：

- **主抽象层：Rig**
- **本地推理层：mistral.rs**
- **MCP：rmcp**
- **OpenAI 直连 SDK 备选：async-openai**
- **向量层优先级：pgvector / qdrant-client，根据部署形态二选一**

### 3.2 原因

#### A. 为什么主框架仍然推荐 Rig

因为你当前要做的是“平台里的统一 AI 能力层”，不是单纯调用一个模型 API。

`Rig` 更适合承担这层职责：

- 统一 completion / embeddings 抽象
- 已内建多 provider 支持
- 有 agent、vector store、RAG 的一体化抽象
- 能自然接入你现在的 `summer-rig` 插件结构

而且 `docs.rs/rig-core` 当前文档里已经明确列出它支持的 provider 和 companion crates，例如：

- provider：Anthropic、Azure、DeepSeek、Gemini、Groq、Ollama、OpenAI、OpenRouter、Perplexity、Together、xAI 等
- vector store companion crates：`rig-qdrant`、`rig-lancedb`、`rig-sqlite`、`rig-mongodb`、`rig-milvus` 等
- companion provider crates：`rig-fastembed` 等

这对一个后台平台项目，比“单厂商 SDK”更有价值。

#### B. 为什么不是只用 mistral.rs

`mistral.rs` 很强，但它主要强在：

- 本地模型推理
- GGUF / LoRA / vision / paged attention / 多模型管理
- 作为本地 inference runtime 使用

它更像“推理后端”或“模型运行时”，不是你整个 AI 平台的最佳上层抽象。

所以它应该是：

- **Rig 的下层补充**
- 不是替代 Rig 的唯一主框架

#### C. 为什么 async-openai 只能作为备选

`async-openai` 的定位很清晰：

- OpenAI 官方 API 的 Rust 非官方实现
- 接口覆盖非常广，包含 Responses、Realtime、Embeddings、Vector stores、Files、Assistants、Administration 等
- 适合做 OpenAI 或 OpenAI-compatible 的直接接入

但它的问题也很明显：

- 它不是多 provider 抽象层
- 它不是 agent / RAG 的统一编排框架
- 如果业务层直接依赖它，后续切厂商或引入多模型路由会更痛

所以它更适合：

- 某些底层 provider adapter
- 某些只需要 OpenAI 兼容接口的独立模块

而不适合作为整个项目的 AI 能力中枢。

---

## 4. LLM 应用 / Agent 框架

### 4.1 第一梯队：适合放进项目主线评估

| 项目 | 定位 | 优点 | 风险 / 不足 | 结论 |
|---|---|---|---|---|
| [Rig](https://github.com/0xPlaygrounds/rig) / [docs.rs](https://docs.rs/rig-core) | Rust 原生 LLM 应用框架 | 多 provider、多 embeddings、vector store companion crates，适合做统一抽象层 | 版本演进较快，接口仍有变化风险 | **首选** |
| [AutoAgents](https://github.com/liquidos-ai/AutoAgents) | 多 Agent 协作框架 | 多 agent、memory、executor 分层比较清晰 | 更偏 multi-agent runtime，不是最轻的接入层 | 可持续关注 |
| [langchain-rust](https://github.com/Abraxas-365/langchain-rust) | LangChain 风格 Rust 框架 | 对 OpenAI/Ollama/Anthropic、embedding、vector store 都有覆盖，API 对 LangChain 用户友好 | 风格更像 LangChain Rust 化，生态整合度不如 Rig | 可作为第二备选 |

### 4.2 第二梯队：有价值，但不建议作为当前主线

| 项目 | 定位 | 观察 |
|---|---|---|
| [llm-chain](https://github.com/sobelio/llm-chain) / [docs.rs](https://docs.rs/llm-chain) | 早期链式 LLM 框架 | 思路完整，但仓库公开发布时间较早，最近几年在主流 Rust LLM 讨论里的中心性不如 Rig |
| [rs-graph-llm / graph-flow](https://github.com/a-agmon/rs-graph-llm) | 类 LangGraph 的图工作流 | 更适合复杂状态机 / 多步骤流程；其 README 里直接把 Rig 作为 LLM 集成层，这个组合思路值得参考 |
| [langchainrust](https://docs.rs/langchainrust) | 新出现的 LangChain 风格 crate | 名称与 `langchain-rust` 接近，生态辨识度和长期稳定性还需要观察 |

### 4.3 这一组对本项目的判断

如果你的目标是：

- 在后台项目里统一接厂商
- 后续可能上工具调用、RAG、Agent
- 不想把业务逻辑直接绑死在 OpenAI 接口形状上

那主线仍然应该是：

- `Rig` 作为统一抽象
- 自己封一层 `summer-rig` 适配层
- 业务代码只依赖你的 trait / registry / plugin，不直接依赖 Rig 细节

---

## 5. 本地推理 / Transformers / 模型运行时

### 5.1 这一组的核心区别

这一组不能和 `Rig` 直接放在同一层比较。

- `Rig` 解决的是“应用抽象和编排”
- `mistral.rs` / `Candle` / `rust-bert` 解决的是“模型怎么在本地跑起来”

### 5.2 重点项目

| 项目 | 定位 | 优点 | 风险 / 不足 | 结论 |
|---|---|---|---|---|
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) / [docs.rs](https://docs.rs/mistralrs) | 本地推理引擎 / SDK | 支持 text、GGUF、LoRA、vision、多模型、paged attention，能力非常强 | 不是统一多 provider 应用框架 | **本地推理首选** |
| [Candle](https://github.com/huggingface/candle) | Hugging Face 的 Rust ML / inference 基座 | 生态强、性能好、很多示例都围绕它展开 | 更底层，需要自己搭很多上层能力 | 底层能力优先关注 |
| [rust-bert](https://github.com/guillaume-be/rust-bert) | Rust Transformers 任务库 | 适合传统 NLP / transformer 推理任务 | 对“现代 hosted LLM 平台层”帮助不如 Rig / mistral.rs 直接 | 适合特定 NLP 任务 |
| [Burn](https://github.com/tracel-ai/burn) | Rust 深度学习框架 | 训练/推理都可以做，工程设计好 | 对你的当前后台项目而言太底层 | 关注即可，不作为当前主线 |
| [Kalosm](https://github.com/floneum/floneum) | 本地 AI 平台能力组合 | 覆盖语言、音频、视觉、结构化生成 | 平台味更重，未必适合你现有插件体系 | 观察项 |

### 5.3 Transformers / tokenizer / ONNX 相关配套

| 项目 | 定位 | 说明 |
|---|---|---|
| [tokenizers](https://docs.rs/tokenizers/latest/tokenizers/) | Hugging Face Rust tokenizer 核心库 | 这是现在最通用、最值得优先考虑的 tokenizer 基础设施 |
| [rust_tokenizers](https://docs.rs/rust_tokenizers) | 老牌 Rust tokenizer 库 | 更偏 `rust-bert` 生态 |
| [ort](https://github.com/pykeio/ort) | Rust ONNX Runtime 绑定 | 很多本地 embedding / rerank / small model 路径会依赖它 |

### 5.4 这一组对本项目的判断

如果你后面要支持：

- 私有化部署
- 离线环境
- 本地 embedding / rerank / 小模型推理

那建议的分层是：

- 上层：`Rig`
- 本地推理：`mistral.rs`
- tokenizer / ONNX / embedding 组件按需引入

而不是让业务代码直接围着 `mistral.rs` 长。

---

## 6. 厂商 SDK / OpenAI 兼容 API 客户端

### 6.1 现状判断

Rust 这块的现状不是“有一个绝对统治级官方 SDK”，而是：

- OpenAI 侧：社区 SDK 相对成熟
- Anthropic / Gemini / 其它 provider：更多依赖各框架内置 provider，或者自己用 `reqwest`
- 所以纯 SDK 路线会更碎

### 6.2 值得看的项目

| 项目 | 定位 | 优点 | 风险 / 不足 | 结论 |
|---|---|---|---|---|
| [async-openai](https://github.com/64bit/async-openai) / [docs.rs](https://docs.rs/async-openai) | OpenAI 非官方 Rust SDK | API 覆盖广，Responses、Realtime、Embeddings、Vector Stores、Files、Assistants、Admin 都有 | 只解决 OpenAI / OpenAI-compatible，不解决多 provider 抽象 | **最佳 SDK 备选** |
| [openai-api-rs](https://docs.rs/crate/openai-api-rs/latest) | OpenAI 非官方客户端 | 版本更新仍在继续 | 文档覆盖很低，工程体验不如 `async-openai` | 不建议优先 |
| `reqwest + 自定义 DTO` | 最原始方案 | 完全可控，适合很窄的接口面 | 维护成本高，重复造轮子 | 仅用于极小范围场景 |

### 6.3 这一组对本项目的判断

如果业务场景是：

- 单厂商
- 接口面非常清晰
- 不需要多 provider 抽象

那 `async-openai` 很合适。

但如果你是要做平台层，SDK 应该处于：

- `Rig` provider adapter 的补充
- 而不是整个能力架构的中心

---

## 7. MCP 生态

### 7.1 主结论

MCP 这条线没有太多悬念：

- **首选 rmcp**

原因很简单：

- [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) 已经是官方 Rust SDK
- [docs.rs/rmcp](https://docs.rs/crate/rmcp/latest) 近期仍在发布新版本
- 你的仓库本身已经在用 `rmcp`

### 7.2 其他值得知道的项目

| 项目 | 定位 | 结论 |
|---|---|---|
| [rmcp](https://github.com/modelcontextprotocol/rust-sdk) / [docs.rs](https://docs.rs/rmcp/latest/index.html) | 官方 Rust MCP SDK | **主线方案** |
| [mcp-framework](https://github.com/koki7o/mcp-framework) | 基于 MCP 的上层框架 | 可以关注，但不要替代 `rmcp` 本身 |
| [rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk) | 社区异步工具包 | 有活跃度，但在官方 SDK 已经明确的前提下，不建议主线切过去 |
| [rust-mcp-schema](https://github.com/rust-mcp-stack/rust-mcp-schema) | MCP schema 实现 | 适合作为 schema 工具层参考，不是你当前核心依赖 |

### 7.3 这一组对本项目的判断

`summer-mcp` 继续站在 `rmcp` 上是对的，重点应该放在：

- 你自己的 tool/resource/prompt 抽象
- 错误模型
- 输出契约
- OpenAPI / CRUD / admin codegen 能力

而不是频繁切 MCP SDK。

---

## 8. AI Gateway / Proxy / Router

这一组和上面不太一样，它们不一定是“库”，很多是“服务型项目”。  
但你明确说了这一组也要纳入，所以这里单独分组。

### 8.1 值得关注的 Rust 项目

| 项目 | 定位 | 优点 | 风险 / 不足 | 结论 |
|---|---|---|---|---|
| [Noveum AI Gateway](https://github.com/Noveum/ai-gateway) | Rust 写的 AI Gateway / Proxy | 多 provider、流式支持、OpenAI-compatible、偏生产代理 | 更像独立网关服务，不是嵌入式应用框架 | 如果你想做独立 AI 网关，值得重点参考 |
| [LLM Link](https://github.com/lipish/llm-link) | 通用 LLM 代理服务，同时可作 Rust library | 对 Codex CLI、Claude Code、Zed 这类 AI coding tool 很友好，多协议支持清晰 | 更偏代理层与工具适配层，不是通用业务应用框架 | 对“统一代理入口”思路很有参考价值 |

### 8.2 这类项目对本仓库的意义

如果你未来要把 AI 能力从“后台内部插件”再抽一层，做成：

- 独立 AI 网关服务
- 多租户 provider 路由
- OpenAI / Anthropic / Ollama 多协议入口
- 统一鉴权、审计、计费、限流

那这类 gateway 项目就很值得研究。

但如果你当前只是做后台能力层，它们应当作为：

- 参考实现
- 或未来拆分出的独立服务方向

不应该替代你现在的 `summer-rig`。

---

## 9. 向量库 / Embedding / RAG 适配层

### 9.1 这组最重要的不是“谁最火”，而是“和你的部署模型是否匹配”

如果你当前后台主库已经是 PostgreSQL，那么：

- `pgvector` 的接入成本最低
- 运维心智负担也最小

如果你想把检索层独立出来，或者后面要规模更大，那么：

- `Qdrant + qdrant-client` 更合理

### 9.2 候选项

| 项目 | 定位 | 结论 |
|---|---|---|
| [pgvector-rust](https://github.com/pgvector/pgvector-rust) / [docs.rs](https://docs.rs/pgvector/latest/pgvector/) | PostgreSQL 向量扩展 Rust 支持 | **如果你想复用 Postgres，这是最自然的首选** |
| [qdrant-client](https://docs.rs/qdrant-client/latest/qdrant_client/) | Qdrant Rust 客户端 | **如果你想把向量检索独立成专业组件，这是优先候选** |
| [fastembed-rs](https://github.com/Anush008/fastembed-rs) | 本地 embedding / rerank Rust 库 | 很适合本地 embedding、轻量 RAG、无 GPU 或 ONNX 路线 |
| [tokenizers](https://docs.rs/tokenizers/latest/tokenizers/) | tokenizer 底层 | 几乎是这条链路上最通用的基础设施之一 |
| `Rig` companion crates | RAG / vector store 适配层 | 如果主线已经选 Rig，优先看 `rig-qdrant`、`rig-lancedb` 等 companion crates，减少自己封装成本 |

### 9.3 对本项目的推荐

当前更建议这样选：

- **短中期**：`pgvector`
- **中长期独立检索服务**：`qdrant-client`
- **本地 embedding / rerank**：`fastembed-rs`

也就是说：

- 不一定一上来就把向量层独立成新服务
- 可以先贴着现有 Postgres 落地
- 等检索规模、召回策略、模型多样性上来，再考虑 Qdrant

---

## 10. 推荐架构落点

如果按这个项目现在的方向，我建议的架构落点是：

### 10.1 当前可执行方案

- 框架主线：`Rig`
- MCP：`rmcp`
- provider 接入：优先走 Rig provider，必要时局部补 `async-openai`
- 本地推理：按需引入 `mistral.rs`
- 向量层：先 `pgvector`，后续可演进到 `Qdrant`

### 10.2 代码分层建议

- `summer-rig`
  - 对外只暴露你自己的 registry / config / provider selector / facade
- `Rig`
  - 只留在适配层
- 业务服务
  - 不直接依赖 `Rig` provider client 类型
- `summer-mcp`
  - 继续走 `rmcp`
- 若未来有统一 AI 出口
  - 单独抽独立 gateway 服务，而不是继续塞进 admin 主进程

### 10.3 最不建议的方案

最不建议的是这两种：

1. 业务代码全面直连 `async-openai`
2. 直接把 `mistral.rs` 当成整套平台抽象层

前者会让多 provider 和后续演进很痛，后者会把“本地推理能力”和“应用抽象层”混在一起。

---

## 11. 一句话选型结论

### 如果只给一句话

- **做后台 AI 能力平台：选 Rig**
- **做本地模型推理：选 mistral.rs**
- **做 MCP：选 rmcp**
- **只调 OpenAI：选 async-openai**
- **先做 RAG 落地：先 pgvector，再视规模考虑 Qdrant**

---

## 12. 附录：本次调研主要来源

### LLM 应用 / Agent 框架

- Rig: https://github.com/0xPlaygrounds/rig
- Rig docs.rs: https://docs.rs/rig-core
- AutoAgents: https://github.com/liquidos-ai/AutoAgents
- langchain-rust: https://github.com/Abraxas-365/langchain-rust
- llm-chain: https://github.com/sobelio/llm-chain
- llm-chain docs.rs: https://docs.rs/llm-chain
- rs-graph-llm / graph-flow: https://github.com/a-agmon/rs-graph-llm

### 本地推理 / Transformers

- mistral.rs: https://github.com/EricLBuehler/mistral.rs
- mistralrs docs.rs: https://docs.rs/mistralrs
- Candle: https://github.com/huggingface/candle
- rust-bert: https://github.com/guillaume-be/rust-bert
- Burn: https://github.com/tracel-ai/burn
- Kalosm / Floneum: https://github.com/floneum/floneum
- tokenizers docs.rs: https://docs.rs/tokenizers/latest/tokenizers/
- ort: https://github.com/pykeio/ort

### 厂商 SDK / API 客户端

- async-openai: https://github.com/64bit/async-openai
- async-openai docs.rs: https://docs.rs/async-openai
- openai-api-rs docs.rs: https://docs.rs/crate/openai-api-rs/latest

### MCP

- official rust-sdk / rmcp: https://github.com/modelcontextprotocol/rust-sdk
- rmcp docs.rs: https://docs.rs/rmcp/latest/index.html
- mcp-framework: https://github.com/koki7o/mcp-framework
- rust-mcp-sdk: https://github.com/rust-mcp-stack/rust-mcp-sdk
- rust-mcp-schema: https://github.com/rust-mcp-stack/rust-mcp-schema

### AI Gateway / Router

- Noveum AI Gateway: https://github.com/Noveum/ai-gateway
- LLM Link: https://github.com/lipish/llm-link

### 向量库 / Embedding / RAG

- qdrant-client docs.rs: https://docs.rs/qdrant-client/latest/qdrant_client/
- pgvector-rust: https://github.com/pgvector/pgvector-rust
- pgvector docs.rs: https://docs.rs/pgvector/latest/pgvector/
- fastembed-rs: https://github.com/Anush008/fastembed-rs

