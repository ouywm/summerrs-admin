# Claw 补遗 + LLM Gateway/Relay 学习路线

> 模型：Claude Opus 4.7 (1M context)，2026-04-18
> 覆盖：新增 Claw 项目 + `docs/relay/` 下 5 种语言 30 个网关项目的学习推荐

---

## Part A：Rust Claw 家族补遗

上次整理后又发现以下 Rust 实现，完整更新：

### A.1 已整理过（略）
IronClaw / ZeroClaw / OpenCrust / openclaw-rs / Claw Code / ClawCR

### A.2 新增 Rust 项目

| 项目 | 定位 | 亮点 | 链接 |
|---|---|---|---|
| **ZeptoClaw** | "final form"，多租户生产级 | 6MB 二进制，33 工具 / 11 通道 / 16 provider / 6 沙箱 runtime | [qhkm/zeptoclaw](https://github.com/qhkm/zeptoclaw) |
| **MicroClaw** | channel-agnostic agent | 启发自 nanoclaw，MCP 支持，Web UI + 多通道共享 runtime | [microclaw/microclaw](https://github.com/microclaw/microclaw) |
| **mini-claw-code** | 📚 **教学项目** | 15 章从零手把手造 coding agent，TDD | [odysa/mini-claw-code](https://github.com/odysa/mini-claw-code) |
| **Moltis** | 生产级自托管 | 27 个 workspace crate，53 个非默认 feature，强观测 | 2026-02-12 发布（Fabien Penso） |
| **RustClaw** | 另一 Rust 替代 | 8.8MB，~0.02s 启动，3.9MB 峰值内存 | [RustClawBot/rustclaw](https://github.com/RustClawBot/rustclaw) |
| **ClawSwarm** | 原生多 Agent | 编译到 Rust，基于 Swarms 生态 | [Swarm-Corp/ClawSwarm](https://github.com/The-Swarm-Corporation/ClawSwarm) |

### A.3 非 Rust 但必提（作为生态参照）

| 项目 | 语言 | 说明 |
|---|---|---|
| **OpenClaw** | TypeScript | 始祖，150K+ stars |
| **NanoClaw** | TypeScript | 容器化，基于 Anthropic Agents SDK |
| **PicoClaw** | **Go** | Sipeed 出品，$10 硬件 <10MB RAM |
| **MiniClaw** | 未知 | Web dashboard + 插件 + 监控 |
| **MetaClaw** | — | "对话即学习"，自进化 |
| **NemoClaw** | — | 声明式策略 + digest 供应链 |

### A.4 更新后的 Rust Claw 学习优先级

```
教学入门：mini-claw-code（15 章，能看到 agent 循环最小实现）
    ↓
第一个生产级：IronClaw（安全优先，6 crate）
    ↓
架构进阶：ZeroClaw（15 crate，trait 驱动）
    ↓
观测生产级：Moltis（27 crate，53 feature）
    ↓
多租户：ZeptoClaw（6 runtime 沙箱）
    ↓
多 Agent：ClawSwarm
```

**Sources**:
- [A Quick Look at Claw-Family (DEV)](https://dev.to/0xkoji/a-quick-look-at-claw-family-28e3)
- [blocmates: Claw Wars - 11 Spin-Offs](https://www.blocmates.com/articles/the-claw-wars)
- [evoailabs Medium: Claw Craziness](https://evoailabs.medium.com/openclaw-nanobot-picoclaw-ironclaw-and-zeroclaw-this-claw-craziness-is-continuing-87c72456e6dc)

---

## Part B：`docs/relay/` —— LLM Gateway/Relay 学习路线

### B.1 先理解这是什么项目

`docs/relay/` 下不是 Claw agent，而是 **LLM API 网关 / 代理 / 路由** 类项目（"AI Gateway"）：统一接入多家 LLM，做**鉴权 / 计费 / 限流 / 缓存 / 路由 / 观测**。

它和 Claw 的关系：**Claw 是 Agent（调用方），Gateway 是中间件（被调方）**。

典型用户场景：
- 企业内部一套 API Key 管理，多团队复用
- 多 Provider 热切换 / failover
- Token 预算和成本报表
- 审计日志（合规）

### B.2 项目分层

我把仓库里 30 个项目按**架构复杂度**和**成熟度**分层：

#### 🥇 Tier 1 — 工业标杆（必学）
| 项目 | 语言 | 学什么 |
|---|---|---|
| **litellm** | Python | 统一 API 抽象的**概念鼻祖**，100+ Provider 覆盖，OpenAI 兼容层 |
| **one-api** | Go | **最火的开源网关**（40K+ star），多租户 / 计费 / 管理后台 |
| **portkey-gateway** | TS | 商业产品开源版，guardrails / 缓存 / 路由策略 |

#### 🥈 Tier 2 — Rust 网关代表（你的主战场）
| 项目 | 定位 | 学什么 |
|---|---|---|
| **noveum/ai-gateway** | 通用网关 | Rust 做网关的工程骨架 |
| **traceloop/hub** | 可观测性集成 | OpenTelemetry 集成 |
| **ultrafast-ai-gateway** | 性能导向 | 极低延迟 + 高并发 |
| **lunaroute** | 路由策略 | 基于内容的路由算法 |
| **anthropic-proxy-rs** | 专用代理 | 只代 Anthropic，最小可运行范例 |
| **claude-code-mux** | Claude Code 多路复用 | 多账号 Claude Code 轮换 |
| **llm-connector / llm-providers** | Provider 抽象库 | 单一关注点：统一接口 |
| **crabllm / hadrian / model-gateway-rs / llmg / unigateway** | 各种尝试 | 看不同作者的取舍 |

#### 🥉 Tier 3 — Go 网关（生态成熟）
| 项目 | 说明 |
|---|---|
| **new-api / one-hub** | one-api 的两大分叉，中文社区最活跃 |
| **bifrost** | Maxim 出品，主打性能 |
| **APIPark** | 商业级 API 管理平台 |
| **CLIProxyAPI** | 代理 CLI 工具调用 |
| **axonhub** | 企业级 hub |
| **proxify** | 安全代理 |

#### Tier 4 — 其他语言
| 项目 | 语言 | 特色 |
|---|---|---|
| **crewAI** | Python | 多 Agent 编排（严格讲不是网关） |
| **claude-code-api** | Python | Claude Code 包成 API |
| **llamaxing / llm-router-api** | Python | 路由策略尝试 |
| **llmgateway / openai-gateway** | TS | TS 小型实现 |
| **solon-ai** | Java | Solon 生态的 AI 扩展 |

### B.3 🎯 推荐学习路线（跨语言）

**核心原则**：**先 Rust 摸清结构 → 横向对比其他语言 → 最后读标杆吸取设计思想**。

#### 阶段 1：Rust 入门网关（1~2 周）

> 你已有 Rust 基础，从**最小可运行**开始看架构。

1. **anthropic-proxy-rs**（最小）
   - 目标：1 天读完
   - 学点：HTTP 代理 / 流式转发 / reqwest + axum 的配合
2. **llm-connector** + **llm-providers**
   - 目标：2 天
   - 学点：**Provider 抽象该怎么拆**，这是所有网关的核心
3. **model-gateway-rs** 或 **unigateway**
   - 目标：3 天
   - 学点：配置驱动路由 + 认证中间件

#### 阶段 2：Rust 进阶网关（2 周）

4. **noveum/ai-gateway**
   - 完整生产特性：限流 / 缓存 / 鉴权 / metrics
5. **traceloop/hub**
   - OpenTelemetry 集成（对将来做 Agent 可观测极有帮助）
6. **ultrafast-ai-gateway**
   - 性能技巧：连接池 / 零拷贝 / 流式处理

#### 阶段 3：横向对比 Go 实现（2 周）

> 学 Go 网关的目的：**看用户真正要的功能长啥样**，因为 Go 生态做得最早最全。

7. **one-api** ⭐
   - **必读**。先跑起来用用管理后台，感受一个成熟网关的全部场景
   - 重点看：渠道（Provider）管理、令牌（Key）分发、计费
8. **new-api** 或 **one-hub**
   - 对比 one-api 的增强方向，看社区真实痛点
9. **bifrost**
   - 性能向 Go 网关的实现，对照 Rust 看差异

#### 阶段 4：标杆项目吸收思想（2~3 周）

10. **litellm**（Python）⭐⭐⭐
    - **AI 网关的概念教科书**。100+ Provider 的抽象设计
    - 读点：`litellm/llms/*.py` 每家的 adapter，对比看通用性
    - 花 1 周
11. **portkey-gateway**（TypeScript）
    - 商业产品的开源版，特性密度最高：guardrails / virtual key / fallback chain / conditional routing
    - 读点：配置文件 DSL 的设计
12. **crewAI**（Python，选读）
    - 如果你对多 Agent 感兴趣

#### 阶段 5：综合产出

**任务**：用 Rust 写你自己的 mini AI gateway，目标 2000 行内，实现：
- 支持 OpenAI / Anthropic / Ollama 3 家
- 支持 OpenAI 协议兼容层（前端可直接用 OpenAI SDK）
- Token 限流 + 简单计费
- 流式转发
- 基础 metrics（prometheus）

参考：llm-connector（抽象）+ anthropic-proxy-rs（实操）+ litellm（功能范围）。

### B.4 各语言推荐学 1 个就够

如果只有精力选一个：

| 语言 | 必学项目 | 理由 |
|---|---|---|
| Rust | **noveum/ai-gateway** 或 **traceloop/hub** | 体系完整，工程参考 |
| Go | **one-api** | 功能最全，生产验证 |
| Python | **litellm** | 概念标杆 |
| TypeScript | **portkey-gateway** | 商业级特性最密集 |
| Java | **solon-ai** | 国产生态代表，供技术选型时对比 |

### B.5 学习顺序（从 Rust 到其他语言）

```
Rust（你的母语）
  ① anthropic-proxy-rs ── 最小可运行
  ② llm-connector ── 抽象建模
  ③ noveum/ai-gateway ── 生产特性
       ↓
Go（生态最成熟）
  ④ one-api ── 功能天花板
  ⑤ bifrost ── 性能技巧
       ↓
Python（概念标杆）
  ⑥ litellm ── 100+ Provider 抽象艺术
       ↓
TypeScript（商业级）
  ⑦ portkey-gateway ── 最密集特性集合
       ↓
Java（选学）
  ⑧ solon-ai ── 国产企业栈
       ↓
💡 自己造轮子（Rust）
   综合以上所有精华，2000 行实现 mini gateway
```

**预计总周期**：8~10 周（每天 2 小时）。

### B.6 阅读每个网关项目时的"5 问模板"

不管什么语言，对一个网关都问这 5 个问题：

1. **Provider 抽象**：trait/interface 怎么定义？如何新增一家？
2. **流式处理**：SSE / WebSocket 怎么做转发？背压如何处理？
3. **鉴权计费**：Token 如何存？限流算法（token bucket / sliding window）？
4. **可观测性**：日志 / metric / trace 怎么打？
5. **错误处理**：上游出错如何降级？重试策略？熔断？

5 个问题都能答出来，就算真正读懂了。

### B.7 与 Claw 的联动

如果你学完 Claw 和 Gateway，可以做一个**端到端项目**：

```
用户 → IronClaw/ZeroClaw (Agent)
          ↓ 发起 LLM 请求
       你自己造的 Mini Gateway
          ↓ 路由
       多家 LLM Provider
```

这就是生产 AI 系统的真实架构。

---

## Part C：总学习路线图（Claw + Gateway 合并版）

```
(0) Rust 基础 ✓
  │
  ├── mini-claw-code (教学，15 章) ─────────── 1 周
  │
  ├── anthropic-proxy-rs + llm-connector ────── 1 周
  │        (gateway 最小实现)
  │
  ├── IronClaw ─────────────────────────────── 4~6 周
  │        (安全 Agent)
  │
  ├── noveum/ai-gateway + traceloop/hub ────── 2 周
  │        (生产级 gateway)
  │
  ├── ZeroClaw ─────────────────────────────── 6~8 周
  │        (trait-driven Agent)
  │
  ├── one-api (Go) ─────────────────────────── 1 周
  │        (gateway 生态标杆)
  │
  ├── litellm (Python) ─────────────────────── 1 周
  │        (Provider 抽象艺术)
  │
  └── 自己造：mini agent + mini gateway ─────── 持续
           (融会贯通)
```

**全程约 4~5 个月，每天 2~3 小时**。

---

## 附录：快速参考表

### Rust 网关项目一览（本仓库 13 个）

| 项目 | GitHub | 一句话 |
|---|---|---|
| ai-gateway | [noveum/ai-gateway](https://github.com/noveum/ai-gateway) | 通用 AI 网关 |
| anthropic-proxy-rs | [m0n0x41d/anthropic-proxy-rs](https://github.com/m0n0x41d/anthropic-proxy-rs) | Anthropic 专用代理 |
| claude-code-mux | [9j/claude-code-mux](https://github.com/9j/claude-code-mux) | Claude Code 多账号复用 |
| crabllm | [crabtalk/crabllm](https://github.com/crabtalk/crabllm) | LLM 工具集 |
| hadrian | [ScriptSmith/hadrian](https://github.com/ScriptSmith/hadrian) | — |
| hub | [traceloop/hub](https://github.com/traceloop/hub) | OTel 集成网关 |
| llm-connector | [lipish/llm-connector](https://github.com/lipish/llm-connector) | Provider 抽象 |
| llm-providers | [lipish/llm-providers](https://github.com/lipish/llm-providers) | Provider 实现库 |
| llmg | [modpotatodotdev/llmg](https://github.com/modpotatodotdev/llmg) | LLM 网关 |
| lunaroute | [erans/lunaroute](https://github.com/erans/lunaroute) | 智能路由 |
| model-gateway-rs | [code-serenade/model-gateway-rs](https://github.com/code-serenade/model-gateway-rs) | 模型网关 |
| ultrafast-ai-gateway | [techgopal/ultrafast-ai-gateway](https://github.com/techgopal/ultrafast-ai-gateway) | 极速网关 |
| unigateway | [EeroEternal/unigateway](https://github.com/EeroEternal/unigateway) | 统一网关 |

### Sources

- [Till Freitag: OpenClaw Alternatives](https://till-freitag.com/en/blog/openclaw-alternatives-en)
- [ScriptByAI: Best OpenClaw Alternatives 2026](https://www.scriptbyai.com/best-openclaw-alternatives/)
- [RustClaw Official](https://www.rustclaw.org/)
- [OpenClaw GitHub Topic](https://github.com/topics/openclaw)