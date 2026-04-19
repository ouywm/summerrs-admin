# ZeroClaw 学习路线（手把手详细版 v2）

> 模型：Claude Opus 4.7 (1M context) · 2026-04-18
> 仓库：[zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw)
> 代码量：**~485,873 行 Rust**（16 crate + apps/ + src/）
> 前置：建议先学完 [IronClaw 学习路线](./ironclaw-learning-path.md)
> 学习模式：**逐日打卡**，每节都有「读什么 / 问什么 / 练什么 / 验什么」

---

## 规模警告

ZeroClaw 代码量是 IronClaw 的 **6.7 倍**（72k vs 485k）。关键原因：
- 30+ channel 实现
- 20+ provider 实现
- 60+ tool 实现
- 记忆 crate 单独 10k+ 行（sqlite.rs 2764 行）
- Tauri 桌面 app
- 硬件 crate（aardvark-sys + robot-kit）

**学习策略：不硬读所有代码**。按分层**抽样精读**：每类挑 1~2 个代表，其余扫过。

---

## 总览目录（12 周计划）

**前置**
- [Week 0 · 环境 & 对比 IronClaw](#week-0--环境--对比-ironclaw)（5 天）

**契约层**
- [Week 1 · zeroclaw-api（2,743 行）](#week-1--zeroclaw-api)（7 天）
- [Week 2 · config + infra + macros + tool-call-parser](#week-2--config--infra--macros--tool-call-parser)（5 天）

**能力层**
- [Week 3 · zeroclaw-providers](#week-3--zeroclaw-providers)（5 天）
- [Week 4-5 · zeroclaw-memory（10,713 行）](#week-4-5--zeroclaw-memory)（10 天）
- [Week 6 · zeroclaw-tools](#week-6--zeroclaw-tools)（6 天）
- [Week 7 · zeroclaw-channels](#week-7--zeroclaw-channels)（5 天）
- [Week 8 · zeroclaw-plugins](#week-8--zeroclaw-plugins)（3 天）

**运行时**
- [Week 9-10 · zeroclaw-runtime](#week-9-10--zeroclaw-runtime)（10 天）
- [Week 11 · gateway + tui + main binary](#week-11--gateway--tui--main-binary)（5 天）

**硬件/桌面（选学）**
- [Week 12 · zeroclaw-hardware + aardvark + robot-kit](#week-12--zeroclaw-hardware--aardvark--robot-kit)（5 天）
- [Week 13 · apps/tauri](#week-13--appstauri)（3 天）

**产出**
- [Week 14+ · 实战 + 性能调优 + 嵌入式部署](#week-14--实战--性能调优--嵌入式部署)

**附录**
- [附录 A：IronClaw → ZeroClaw 迁移表](#附录-aironclaw--zeroclaw-迁移表)
- [附录 B：Feature Gate 分层](#附录-bfeature-gate-分层)
- [附录 C：常见坑](#附录-c常见坑)

---

## Week 0 · 环境 & 对比 IronClaw

### Day 1：装环境

**🛠 练什么**
```bash
# 基础
rustup default stable
rustc --version  # >= 1.87

# Justfile 支持
cargo install just

# Tauri（可选）
brew install node pnpm
npm install -g @tauri-apps/cli

# Qdrant（可选，学向量后端用）
docker run -d -p 6333:6333 --name qdrant qdrant/qdrant

# 克隆 + 构建
git clone https://github.com/zeroclaw-labs/zeroclaw.git ~/code/zeroclaw
cd ~/code/zeroclaw
just build   # 或 cargo build --release --locked
```

**⚠️ 警告**：首次编译 **30~60 分钟**（比 IronClaw 慢 2 倍，因为 Crate 更多）。挂着做别的事。

---

### Day 2：onboard + 首跑

**🛠 练什么**
```bash
./target/release/zeroclaw onboard
./target/release/zeroclaw

# 另开一屏看日志
RUST_LOG=zeroclaw=debug ./target/release/zeroclaw
```

**❓ 问什么**
1. onboard 写了什么到 `~/.zeroclaw/`？
2. secrets 文件怎么加密的？
3. 默认 memory 后端是什么？默认 provider？

---

### Day 3：15 个 crate 职责速查

**🛠 练什么**
```bash
cd ~/code/zeroclaw
find crates -name "*.rs" -exec wc -l {} + | sort -n | tail -5
cargo depgraph --workspace-only | dot -Tpng > deps.png
```

**❓ 问什么（填表，Day 3 必须能答）**

| crate | 职责 | 行数 | 层级 |
|---|---|---|---|
| zeroclaw-api | ? | 2,743 | 契约 |
| zeroclaw-config | ? | ? | 契约 |
| zeroclaw-infra | ? | ? | 契约 |
| zeroclaw-macros | ? | ? | 契约 |
| zeroclaw-tool-call-parser | ? | ? | 契约 |
| zeroclaw-providers | ? | ? | 能力 |
| zeroclaw-memory | ? | 10,713 | 能力 |
| zeroclaw-tools | ? | ? | 能力 |
| zeroclaw-channels | ? | ? | 能力 |
| zeroclaw-plugins | ? | ? | 能力 |
| zeroclaw-runtime | ? | 巨大 | 运行时 |
| zeroclaw-gateway | ? | ? | 运行时 |
| zeroclaw-hardware | ? | ? | 运行时 |
| zeroclaw-tui | ? | ? | UI |
| aardvark-sys | ? | ? | 硬件 FFI |
| robot-kit | ? | ? | 硬件抽象 |

---

### Day 4：对比 IronClaw

**📖 读什么**
- 本文档附录 A（迁移表）
- 再扫一遍你的 IronClaw 笔记

**❓ 问什么**
1. ZeroClaw 为什么不做 WASM 沙箱？（提示：trait-driven 的替代安全模型）
2. 为什么 memory 拆成 20+ 文件？（提示：多后端 + 卫生层 + 决策层）
3. 为什么 provider 有 20+ 个？（提示：OpenAI 兼容 + reliable/router 装饰器）

---

### Day 5：身份文件 + Feature Gate

**🛠 练什么**
```bash
# 看所有 feature
cat Cargo.toml | grep -A 200 "^\[features\]"

# 看 feature 传播
cargo tree -p zeroclawlabs -e features --depth 2 | head -50
```

**❓ 问什么**
1. 默认 feature 有哪些？
2. `minimal` / `embedded` / `full` 这类 feature 分别打开什么？
3. 哪个 feature 关闭就能省下最多体积？

**✅ Week 0 验什么**
- 能 2 分钟讲清 15 crate 职责
- 能从 Cargo.toml 手搓出一个最小 feature 组合

---

## Week 1 · zeroclaw-api

**目标**：这是 ZeroClaw 的"宪法"——所有 trait 定义在此。**必须 100% 读完**。

### Day 6：lib.rs + agent.rs + media.rs + tool.rs（简单模块）

**📖 读什么**
- `api/src/lib.rs`（36 行）
- `api/src/agent.rs`（17 行）
- `api/src/media.rs`（56 行）
- `api/src/tool.rs`（43 行）

**❓ 问什么**
1. lib.rs 的 `pub use` 暴露了哪些根类型？
2. `tool.rs` 的 `Tool` trait 签名？（一定比 43 行更精华）

---

### Day 7：channel.rs（250 行）

**📖 读什么**
- `api/src/channel.rs` 全文

**❓ 问什么**
1. `Channel` trait 的 recv/send 签名？是否支持流式？
2. `Channel` 是 `Send + Sync` 吗？（影响并发模型）
3. 消息类型 `IncomingMessage` / `OutgoingMessage` 字段？

---

### Day 8：runtime_traits.rs（142 行）+ peripherals_traits.rs（75 行）

**❓ 问什么**
1. `Runtime` 这个 trait 定义了什么？（提示：可能是整个 Agent 的抽象）
2. `Peripherals` trait 是什么？为硬件而设？

---

### Day 9：memory_traits.rs（323 行）

**📖 读什么**
- `api/src/memory_traits.rs` 全文

**❓ 问什么**
1. 核心 `Memory` trait 的方法签名？
2. 是否还有 `MemoryStore` / `VectorStore` / `KnowledgeGraph` 等细分 trait？
3. 如何支持多后端？（提示：trait + dyn）

**🛠 练什么**
把所有 memory trait 方法抄到笔记里，每个方法写一句"输入/输出"。

---

### Day 10：observability_traits.rs（324 行）

**📖 读什么**
- `api/src/observability_traits.rs` 全文

**❓ 问什么**
1. 日志 / metric / trace 三件套分别对应哪些 trait？
2. 如何与 OpenTelemetry 集成？

---

### Day 11：provider.rs（633 行，**核心**）

**📖 读什么**
- `api/src/provider.rs` 全文

**❓ 问什么**
1. `Provider` trait 的 chat / stream 方法？
2. `ChatRequest` / `ChatResponse` 字段？（至少列 10 个）
3. 工具调用 IR 的表达？
4. Reasoning/Thinking 字段如何表达？（Anthropic/DeepSeek 特性）
5. Multi-modal（图片/PDF）入参怎么建模？

---

### Day 12：schema.rs（844 行，**最大**）

**📖 读什么**
- `api/src/schema.rs` 全文（分上下午读）

**❓ 问什么**
1. 这里定义了哪些"wire format 无关"的类型？
2. serde 标签策略（`tag` / `content` / `untagged`）各在哪用？
3. 和 provider.rs 的界限？

**✅ Week 1 验什么**
- 能画出 "Provider ↔ Channel ↔ Memory ↔ Tool" 四大 trait 交互图
- 能给任一 trait 手写一个 **mock 实现**（用来测试）
- **必须做**：写一个 50 行的 `mock-provider` crate 实现 Provider trait，返回固定响应

---

## Week 2 · config + infra + macros + tool-call-parser

### Day 13-14：zeroclaw-config

**📖 读什么**
- `config/src/lib.rs` → 其余文件

**❓ 问什么**
1. TOML schema 是什么样？字段层次？
2. 热重载怎么做？（`notify` crate？）
3. Secrets 加密：密钥从哪读？用什么算法？（AES / ChaCha20?）
4. 多环境（dev/prod）如何切？

**🛠 练什么**
给自己的 `~/.zeroclaw/config.toml` 加一个自定义字段，读取并打印。

---

### Day 15：zeroclaw-infra

**📖 读什么**
- `infra/src/lib.rs` → debounce / session / watchdog

**❓ 问什么**
1. Debounce 合并消息的窗口时长？配置在哪？
2. Session 上下文怎么组织？
3. Stall watchdog 的超时策略？LLM 卡死后怎么唤醒？

---

### Day 16：zeroclaw-macros

**📖 读什么**
- `macros/src/lib.rs`
- 挑一个宏，`cargo expand` 看展开

**🛠 练什么**
```bash
cargo install cargo-expand
cargo expand -p zeroclaw-tools <函数名>
```

**❓ 问什么**
1. `#[tool]` 宏的输入是什么？展开成什么？
2. 宏如何自动生成 JSON Schema？

---

### Day 17：zeroclaw-tool-call-parser

**📖 读什么**
- `tool-call-parser/src/*.rs`

**❓ 问什么**
1. 支持多少种格式？（OpenAI tool_calls / Anthropic tool_use / Gemini functionCall / XML / JSON markdown）
2. 状态机如何容错？（部分输出、格式错误）
3. 性能？流式 vs 批量？

**🛠 练什么**
造 10 条真实 LLM 输出 fixture（一半正确、一半半截），测试解析器。

**✅ Week 2 验什么**
- config 能热重载
- 自己写一个 tool，验证 macros 生成正确
- parser 对 10 条 fixture 全部正确处理

---

## Week 3 · zeroclaw-providers

**20+ provider 文件**。别全读，按代表精读：

### Day 18：traits.rs + lib.rs

**📖 读什么**
- `providers/src/traits.rs` + `lib.rs`

**❓ 问什么**
和 `api/src/provider.rs` 的关系？（提示：traits.rs 是 runtime-side 的扩展）

---

### Day 19：openai.rs（经典实现）

**📖 读什么**
- `providers/src/openai.rs` 全文

**❓ 问什么**
1. HTTP 请求构造？
2. 流式解析（SSE）？
3. 错误映射（401/429/500）？
4. 重试策略在这里做还是外层？

---

### Day 20：anthropic.rs + ollama.rs（对比）

**📖 读什么**
- `anthropic.rs`（有 Reasoning）
- `ollama.rs`（本地）

**❓ 问什么**
1. Anthropic 的 thinking blocks 怎么暴露给上层？
2. Ollama 和 OpenAI 的差异（流式格式 / token 计费）？

---

### Day 21：compatible.rs + router.rs + reliable.rs（装饰器三件套）

**📖 读什么**
- `compatible.rs`：OpenAI 兼容网关通用实现
- `router.rs`：多 provider 路由
- `reliable.rs`：重试/熔断包装

**❓ 问什么**
1. 装饰器如何嵌套？`Router(Reliable(OpenAI), Reliable(Anthropic))`？
2. Router 的选择策略：轮询 / 权重 / 成本？

---

### Day 22：挑 3 个特殊 provider

**📖 读什么**
- `bedrock.rs`（AWS 签名）
- `azure_openai.rs`（资源 ID）
- `copilot.rs`（GitHub 认证）

**🛠 练什么**
实现 Kimi 或 Doubao provider。参考 openai.rs 改 URL + auth。

**✅ Week 3 验什么**
- 新 provider 通过一次完整对话
- 能解释 Router 的选择策略
- 能绘制"装饰器栈"图（reliable + router + 原 provider）

---

## Week 4-5 · zeroclaw-memory

**10,713 行**，单个 `sqlite.rs` 就 2,764 行。分 10 天。

### Day 23：lib.rs + traits.rs + backend.rs（总览）

**📖 读什么**
- `memory/src/lib.rs`（668 行）
- `memory/src/traits.rs`（1 行！——所有 trait 在 api crate）
- `memory/src/backend.rs`（158 行）

**❓ 问什么**
1. lib.rs 的主导出？`Memory` / `MemoryLayer` / `MemoryBuilder`？
2. `Backend` trait 和 api 的 `Memory` trait 关系？
3. 如何选择后端？（注册 + config）

---

### Day 24：chunker.rs + embeddings.rs

**📖 读什么**
- `chunker.rs`（377 行）
- `embeddings.rs`（358 行）

**❓ 问什么**
1. Chunking 策略：固定长度 / 段落 / 语义 / token 边界？
2. Embedding 调哪家？支持本地模型吗？
3. 批量 embedding 的并发度？

---

### Day 25：importance.rs + decay.rs + conflict.rs

**📖 读什么**
- `importance.rs`（107 行）：重要性评分
- `decay.rs`（151 行）：时间衰减
- `conflict.rs`（174 行）：冲突消解

**❓ 问什么**
1. 重要性 0~10 的公式？
2. Decay 是指数还是线性？半衰期多长？
3. Conflict 检测：语义相反 / 事实矛盾？怎么合并？

**🛠 练什么**
调 decay 参数（加快衰减 10 倍），观察记忆召回变化。

---

### Day 26：consolidation.rs + hygiene.rs

**📖 读什么**
- `consolidation.rs`（231 行）：合并相关记忆
- `hygiene.rs`（586 行）：记忆卫生（去重 / 归一化）

**❓ 问什么**
1. Consolidation 如何决定"这堆记忆该合并"？（聚类 + 摘要）
2. Hygiene 的定时任务：每天跑一次？凌晨？

---

### Day 27：retrieval.rs（266 行）

**📖 读什么**
- `retrieval.rs`

**❓ 问什么**
1. 混合检索（FTS + vector + graph）的权重？
2. Top-K 的默认值？
3. 分页怎么做？

---

### Day 28：vector.rs（403 行）

**📖 读什么**
- `vector.rs`

**❓ 问什么**
1. 向量距离：cosine / L2 / dot product？
2. 索引：HNSW / IVFFlat / 暴力？
3. 对比 pgvector 和 qdrant 的差异？

---

### Day 29：knowledge_graph.rs（863 行）

**📖 读什么**
- `knowledge_graph.rs`

**❓ 问什么**
1. KG 节点 / 边的 schema？
2. 如何从记忆中抽取实体和关系？（LLM 抽取 or 规则）
3. 和向量检索如何配合？

**🛠 练什么**
对着一个真实对话，手工画出应该产生的 KG，再跑代码看机器产生的是否一致。

---

### Day 30：sqlite.rs（2,764 行，**单文件最大**）

**📖 读什么**（分上午/下午/晚上三段）
- 前 1/3：schema 定义 + migration
- 中 1/3：CRUD 实现
- 后 1/3：复杂查询（FTS + vector）

**❓ 问什么**
1. 用的是 `rusqlite` 还是 `sqlx`？
2. 向量扩展怎么用？（`sqlite-vec` 或 `sqlite-vss`）
3. FTS5 的 tokenizer？中文怎么办？
4. 事务粒度？

---

### Day 31：qdrant.rs（669 行）+ markdown.rs（399 行）+ none.rs（95 行）

**📖 读什么**
- `qdrant.rs`：Qdrant 后端
- `markdown.rs`：纯文件后端（无 DB）
- `none.rs`：no-op 后端（禁用记忆）

**❓ 问什么**
1. 3 种后端的成本权衡？（磁盘 / RAM / 查询速度）
2. 同一套 trait 下，实现差异最大的是哪个方法？

---

### Day 32：lucid.rs + response_cache.rs + snapshot.rs + audit.rs + namespaced.rs + policy.rs

**📖 读什么**
- `lucid.rs`（724 行）：推测是语义层 / "lucid memory"
- `response_cache.rs`（526 行）：LLM 响应缓存
- `snapshot.rs`（470 行）：记忆快照 / 导入导出
- `audit.rs`（293 行）：审计日志
- `namespaced.rs`（232 行）：命名空间隔离
- `policy.rs`（198 行）：记忆策略

**❓ 问什么**（抽样）
- lucid 比普通 memory 多了什么？
- response_cache 的 key 是什么？（prompt hash + params）
- snapshot 的格式：JSON / SQLite / Markdown？

**✅ Week 4-5 验什么**
- 能画 memory 完整架构图（后端 / 卫生层 / 决策层 / 检索层 / 缓存层）
- 能独立实现一个新后端（比如 **redis-vector**）
- 手写 decay 和 importance 公式，对比代码

---

## Week 6 · zeroclaw-tools

**60+ 文件**。别读完，按类别抽样：

### Day 33：mod.rs + 最简单的工具

**📖 读什么**
- `tools/src/lib.rs`
- `calculator.rs`（最简单）
- `weather_tool.rs`

**❓ 问什么**
1. Tool 注册机制？
2. JSON Schema 怎么生成？（手写 / 宏 / serde_json_schema）
3. 错误返回格式？

---

### Day 34：HTTP & Web 类

**📖 读什么**
- `http_request.rs`
- `web_fetch.rs`
- `web_search_tool.rs`

---

### Day 35：文件系统类

**📖 读什么**
- `file_edit.rs`
- `file_write.rs`
- `glob_search.rs`
- `content_search.rs`

**❓ 问什么**
- 路径安全检查在哪？workspace 限制？
- glob 的语法支持？

---

### Day 36：Memory 类（7 个）

**📖 读什么**
- `memory_store.rs` / `memory_recall.rs` / `memory_forget.rs` / `memory_export.rs` / `memory_purge.rs`

---

### Day 37：MCP 4 件套

**📖 读什么（重点）**
- `mcp_protocol.rs`（JSON-RPC）
- `mcp_transport.rs`（stdio / SSE / WS）
- `mcp_client.rs`
- `mcp_tool.rs`

**❓ 问什么**
1. MCP 是 Anthropic 提出的标准，与 ZeroClaw 自己的 tool 抽象怎么融合？
2. stdio 传输和 SSE 的性能差异？
3. 如何动态加载一个 MCP 服务器？

---

### Day 38：AI 嵌套调用类

**📖 读什么**
- `claude_code_runner.rs`
- `codex_cli.rs`
- `gemini_cli.rs`
- `llm_task.rs`

**❓ 问什么**
Agent 套 Agent 的模式：父 Agent 调 Claude Code 作为"工具"，状态如何传递？

**🛠 练什么**
实现 **bilibili_search** 工具（查视频），通过 MCP 暴露。

**✅ Week 6 验什么**
- 新工具在 REPL 可用
- 能用 MCP 从外部（Claude Desktop）调用 ZeroClaw 的工具
- 能讲清 MCP 4 件套分工

---

## Week 7 · zeroclaw-channels

**30+ channel**。核心 5 个精读，其余扫过。

### Day 39：lib.rs + cli.rs（最简单）

---

### Day 40：webhook 风格：telegram + slack + lark

**❓ 问什么**
1. 三家 webhook 签名验证差异？
2. 机器人权限模型？

---

### Day 41：长连接：discord + matrix + irc

**❓ 问什么**
1. 心跳策略？
2. 断线重连？
3. Gateway 事件类型？

---

### Day 42：语音 channel

**📖 读什么**
- `voice_wake.rs`（唤醒词）
- `transcription.rs`（STT）
- `tts.rs`
- `voice_call.rs`

**❓ 问什么**
端到端：唤醒 → STT → Agent → TTS → 播放，各环节延迟？

---

### Day 43：whatsapp_web.rs（硬核）+ orchestrator/

**📖 读什么**
- `whatsapp_web.rs`：浏览器自动化
- `orchestrator/`：多 channel 编排

**🛠 练什么**
实现 **飞书**（Lark 是国际版，飞书是中国版，已有 lark.rs，对照加一个 `feishu.rs` 适配中国 API）。

**✅ Week 7 验什么**
- 新 channel 收发消息
- 能讲清 webhook / 长连接 / 浏览器桥接 三种模式

---

## Week 8 · zeroclaw-plugins

### Day 44：插件发现机制

**📖 读什么**
- `plugins/src/lib.rs`
- 所有子模块

**❓ 问什么**
1. 插件元数据格式？
2. 动态加载方式：动态库 / 进程 / MCP？
3. 与 tools 的区别？

---

### Day 45：加载生命周期 + 沙箱

**❓ 问什么**
1. 启动时扫描 → 验证 → 注册
2. 卸载如何保证干净？
3. 插件崩溃如何隔离？

---

### Day 46：实战：写一个插件

**🛠 练什么**
写一个"每天新闻摘要"插件，含：
- cron 调度
- 调 MCP 工具抓新闻
- 写 memory
- 发到 Telegram channel

**✅ Week 8 验什么**
- 插件跑 3 天不崩
- 能解释 plugin 和 tool / skill 的差异

---

## Week 9-10 · zeroclaw-runtime

**最大的 crate**。21+ 子目录，分 10 天。

### Day 47：目录导览

```
runtime/src/
├── agent/              # 主循环
├── approval/           # 二次确认
├── cost/               # 成本追踪
├── cron/               # 定时任务
├── daemon/             # 守护进程
├── doctor/             # 诊断
├── firmware/           # 固件（OTA？）
├── health/             # 健康检查
├── heartbeat/          # 心跳
├── hooks/              # 生命周期钩子
├── identity/           # 身份
├── integrations/       # 第三方集成
├── nodes/              # 多节点
├── observability/      # 观测
├── onboard/            # onboard 流程
├── platform/           # 平台抽象
├── rag/                # 检索增强
├── routines/           # 周期例程
├── security/           # 安全
├── service/            # 服务入口
├── skillforge/         # 技能工厂
├── skills/             # 技能
├── sop/                # 标准流程
├── tools/              # 工具（runtime 层，22+ 文件）
├── trust/              # 信任
├── tunnel/             # 内网穿透
└── verifiable_intent/  # 可验证意图
```

**🛠 练什么**
`find crates/zeroclaw-runtime/src -name "*.rs" -exec wc -l {} +` 统计 top 20 大文件。

---

### Day 48-49：agent/ + service/ + daemon/

这是主循环入口。对照 IronClaw 的 `runtime/` 读。

**❓ 问什么**
1. 主循环用哪种 async 模型？（`tokio::select!` / actor / stream pipeline）
2. 多 channel 并发处理如何调度？
3. 优雅关机？

---

### Day 50：security/ + trust/

**❓ 问什么**
1. 命令 allowlist（git / npm / cargo）实现在哪？
2. Workspace scope 如何限制文件访问？
3. Pairing 机制（新连接必须配对）细节？
4. Trust 等级划分？（对比 IronClaw 的 Installed/Trusted）

---

### Day 51：approval/ + verifiable_intent/

**❓ 问什么**
1. Verifiable intent 的输出格式？
2. 用户如何批准 / 拒绝？
3. 这是比单纯 capability 更进一步的防御

---

### Day 52：sop/ + routines/ + cron/

**❓ 问什么**
1. SOP 是什么？（标准操作流程，预定义流程代替 LLM 自由发挥）
2. Routines vs Cron 区别？（周期 vs 定时）
3. 怎么组合？

**🛠 练什么**
写一个 SOP："每天早上 8 点生成前一天总结并发 Telegram"。

---

### Day 53：cost/ + observability/ + health/ + doctor/

**❓ 问什么**
1. Cost 如何归因？（per-conversation / per-provider / per-day）
2. 观测 3 件套怎么集成：logs / metrics / traces
3. Doctor 跑哪些检查？

---

### Day 54：hooks/ + integrations/ + nodes/

**❓ 问什么**
1. Hooks 生命周期点：pre-tool / post-tool / pre-llm / post-llm / error？
2. Integrations 目录有哪些第三方？
3. Nodes 多节点什么意思？分布式 Agent？

---

### Day 55：rag/ + skills/ + skillforge/

**❓ 问什么**
1. RAG 相比 memory 层多了什么？
2. Skillforge 是"造技能的技能"？自反射机制？

---

### Day 56：tunnel/ + platform/ + firmware/

**❓ 问什么**
1. Tunnel：ngrok / cloudflared / frp？
2. Platform 抽象哪些平台差异？
3. Firmware 是嵌入式 OTA？

---

**✅ Week 9-10 验什么**
- 能画 runtime 完整架构图
- 能跑通一个 SOP
- 能挂 3 个 hook（pre/post tool + error）
- 能解释 verifiable_intent 的安全意义

---

## Week 11 · gateway + tui + main binary

### Day 57-58：zeroclaw-gateway

**📖 读什么**
- `gateway/src/*.rs`
- `build.rs`（看是否嵌前端资源）

**❓ 问什么**
1. HTTP 框架：axum / actix / hyper？
2. webhook 路由如何动态注册？
3. 签名验证 middleware？

---

### Day 59-60：zeroclaw-tui

**📖 读什么**
- `tui/src/widgets.rs`（255 行）+ 其他

对比 IronClaw TUI 差异。

---

### Day 61：主 binary（src/）

**📖 读什么**
- `src/main.rs` → `src/lib.rs` → 其余

**❓ 问什么**
1. main 如何根据 feature 选择组件？
2. CLI 子命令：onboard / start / doctor / ...？

**✅ Week 11 验什么**
- 能改一个 CLI 子命令的行为
- TUI 加一个自定义 widget

---

## Week 12 · zeroclaw-hardware + aardvark + robot-kit

**可选**。不玩嵌入式可跳过。

### Day 62：zeroclaw-hardware

**📖 读什么**
- `hardware/src/*.rs`

**❓ 问什么**
1. USB 发现如何实现？（`rusb`？）
2. Serial 串口：`serialport` crate？
3. GPIO：平台相关（linux sysfs / embedded-hal）？

---

### Day 63：aardvark-sys

**📖 读什么**
- `aardvark-sys/vendor/`（官方 C SDK）
- `aardvark-sys/src/*.rs`（FFI 绑定）
- `aardvark-sys/build.rs`

**❓ 问什么**
1. bindgen 还是手写 bindings？
2. 如何处理 C 端错误码？

---

### Day 64：robot-kit

**📖 读什么**
- `robot-kit/README.md`
- `robot-kit/PI5_SETUP.md`
- `robot-kit/SOUL.md`（机器人专用身份）
- `robot-kit/src/*.rs`

**❓ 问什么**
1. 机器人控制 API：运动学？姿态？
2. 和主 Agent 如何交互？（作为一个 channel？tool？）

---

### Day 65：实战（有树莓派）

**🛠 练什么**
按 PI5_SETUP 跑：
1. 交叉编译
2. 部署
3. 让 Agent 点亮 LED
4. 让 Agent 读温度传感器

**✅ Week 12 验什么**
- 至少完成虚拟机上的 unit test
- （加分）真机跑通 LED 控制

---

## Week 13 · apps/tauri

### Day 66：Tauri 概览

**📖 读什么**
- [Tauri 官方 getting-started](https://tauri.app/)
- `apps/tauri/` 结构

---

### Day 67：Rust ↔ JS 通信

**❓ 问什么**
1. `invoke` / `emit` / `listen` 各自场景？
2. 如何把 runtime 的 AppEvent 推送到前端？

---

### Day 68：实战

**🛠 练什么**
改 Tauri 前端，加一个"记忆浏览器"页面。

**✅ Week 13 验什么**
- Tauri 桌面 app 跑起来
- 能在 JS 和 Rust 间双向通信

---

## Week 14+ · 实战 + 性能调优 + 嵌入式部署

### 实战项目（选 2~3 个）

**P1（1 周）**：给 memory 加一个新后端（redis-vector / libsql）
**P2（1 周）**：实现 2 个新 channel（飞书 + Matrix）
**P3（2 周）**：给 runtime 加一个 SOP DSL 引擎
**P4（2 周）**：实现 OTA 固件升级（zeroclaw-hardware）
**P5（1 月）**：把二进制压到 3MB 以内（现 3.4MB）

### 性能调优

**体积**
```bash
cargo install cargo-bloat
cargo bloat --release --crates | head -30  # 看哪个依赖吃体积
cargo bloat --release | head -30            # 看哪个函数吃体积
```

常见手段：
- `regex` → `regex-lite`
- `reqwest` → `ureq` 或 `hyper` 直接用
- `serde_json` → `serde-json-core`（no_std）
- 关闭 feature：`--no-default-features --features minimal`
- `[profile.release] lto = "fat"` + `codegen-units = 1` + `strip = "symbols"` + `panic = "abort"`

**启动时间**
```bash
cargo install samply
samply record ./target/release/zeroclaw
```
重点看配置加载、provider 初始化、DB 连接的懒加载机会。

**内存**
- `Arc<T>` 降为 `&T`
- 预分配的 `Vec::with_capacity` 别过大
- 用 `tokio::runtime::Builder::new_current_thread()` 而非 multi_thread

### 嵌入式部署

**交叉编译到 ARM**
```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release \
  --target aarch64-unknown-linux-gnu \
  --no-default-features \
  --features "minimal"
```

**最小 feature 组合（示意）**
```toml
minimal = [
    "providers/openai",
    "memory/sqlite",
    "channels/cli",
]
```

**RISC-V / ESP32**
难度大，看 `docs/hardware/` 里的教程。

---

## 附录 A：IronClaw → ZeroClaw 迁移表

学 IronClaw 时的概念在 ZeroClaw 的对应位置：

| IronClaw 概念 | ZeroClaw 对应 |
|---|---|
| Agent 主循环 `ironclaw_engine/runtime/` | `zeroclaw-runtime/agent/` |
| Capability 系统 | 命令 allowlist + workspace scope + pairing |
| WASM 沙箱 (`src/sandbox/`) | 无 WASM；靠 trait 隔离 + 命令 allowlist |
| 工具 = WASM skill | 工具 = Rust impl Tool（更快，但无语言隔离） |
| PG + pgvector 单一 | SQLite/Qdrant/Markdown/None/Vector 多后端 |
| RRF 混合检索 | retrieval.rs + vector.rs + knowledge_graph |
| Telegram/Discord/Slack/WhatsApp 4 通道 | 30+ 通道 |
| `src/llm/` 8 家 provider | `providers/` 20+ |
| `src/tunnel/` | `runtime/tunnel/` |
| 身份文件（AGENTS/SOUL/USER/IDENTITY） | 类似，文件名可能略不同 |
| `ironclaw_safety`（5 模块） | `runtime/security` + `runtime/trust` + `runtime/approval` + `runtime/verifiable_intent` |
| Heartbeat | `runtime/heartbeat/` |

---

## 附录 B：Feature Gate 分层

（推测，实际以 `Cargo.toml` 为准）

```
minimal   → api + config + providers(openai) + memory(sqlite) + channels(cli)
desktop   → minimal + tui + tauri + gateway
server    → minimal + gateway + channels(telegram/slack/discord)
embedded  → api + providers(ollama) + memory(none) + channels(cli) + hardware
full      → all
```

---

## 附录 C：常见坑

1. **首次编译 30~60 分钟**：挂着睡一觉
2. **feature 冲突** → `cargo tree -e features` 排查
3. **Tauri 起不来** → `cd apps/tauri && pnpm install && pnpm tauri dev`
4. **硬件 feature 默认关**：`--features hardware` 才编
5. **Qdrant 连不上**：检查 `http://localhost:6333/collections`
6. **新 Provider 跑不通**：装饰器栈的顺序（Reliable 必须包裹原始 Provider）
7. **channel 长连接断开**：检查重连逻辑，通常在 `orchestrator/`
8. **内存衰减过快**：调 `decay.rs` 的半衰期参数
9. **向量检索结果怪**：先检查 embedding 维度 / 距离度量
10. **MCP 加载失败**：先用 `npx @modelcontextprotocol/inspector` 排查 MCP server 本身
11. **CI fmt/clippy 挂**：ZeroClaw 严格，nightly fmt + all-targets clippy
12. **crate 发布失败**：`publish = false` 全部 crate（见 Cargo.toml 注释）

---

## 完成判据（必做自测）

学完打勾：

### 理论
- [ ] 口述 15 crate 分层 + 职责（2 分钟）
- [ ] 能写 `zeroclaw-api` 里 6 个核心 trait 的签名
- [ ] 能解释 trait-driven 架构相对 WASM 沙箱的取舍
- [ ] 能讲 RRF + decay + consolidation + knowledge_graph 四个记忆机制如何配合
- [ ] 能讲 SOP / verifiable_intent / approval / trust 四层安全如何协作

### 实战
- [ ] 写过一个 `#[tool]` 宏工具
- [ ] 替换过至少 1 个 Provider / Memory / Channel 实现
- [ ] 跑通一个 SOP
- [ ] 用最小 feature 编出 <3MB 二进制
- [ ] 给项目贡献过至少 1 个 PR（新 provider / 新 channel / 文档）

全部勾 → 你不光懂 ZeroClaw，更懂 **"怎么设计一个可替换、可嵌入、可扩展的 Rust Agent 架构"**。

---

## 下一步（学完 ZeroClaw 之后）

1. **深挖一层**：挑 Provider / Memory / Tool / Channel 中你最感兴趣的，**读完所有实现**，写长文博客
2. **造轮子**：用 `genai` + `Rig` + `axum` 写你自己的 mini-Claw（5k 行内）
3. **回馈社区**：
   - 新 Provider（中国模型：Kimi / Doubao / GLM / Qwen）
   - 新 Channel（飞书 / Keybase / Revolt）
   - 中文文档翻译
   - Bug fix
4. **研究方向**：
   - MCP 生态：写一个通用 MCP server 框架
   - Agent 评测：benchmark 不同 provider 在同一任务上的表现
   - 本地模型：整合 Kalosm，做完全离线 Agent
   - Agent 安全：发 CVE / 做 red team 工具
   - 多 Agent 编排：SOP 引擎做图灵完备

至此，你已进入 Rust AI Agent 的顶级梯队。🦀