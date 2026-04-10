# 07 — 实施路线图

## 一、总体策略：渐进式重构

```
                                          当前
                                            │
Phase 0: 准备                               │ ← 你在这里
Phase 1: core 基础重构                      │   (~1 周)
Phase 2: hub relay 核心域 DDD               │   (~2 周)
Phase 3: hub 管理域 DDD                     │   (~1 周)
Phase 4: core adapter 深度重构              │   (~1 周)
Phase 5: 清理与优化                         │   (~3 天)
                                            ▼
```

**原则**：
- 每个 Phase 结束后系统可编译、可运行
- 不做大规模并行重构，一次只动一个子系统
- 保持旧代码在 `_backup` 中作为参考
- 每个 Phase 有明确的"完成标准"

---

## Phase 0: 准备（当前阶段）

### 已完成 ✅
- [x] DDD 学习文档编写 (`docs/ddd/`)
- [x] Hub DDD 骨架搭建 (domain/application/infrastructure/interfaces)
- [x] guardrail_config 作为 DDD 样板完成
- [x] core 现状诊断和目标架构设计（本系列文档）

### 待完成
- [ ] 确认 DDD 分层规范（团队/个人认可）
- [ ] 确认 core 重构优先级
- [ ] 准备集成测试基础设施

**完成标准**：规划文档 review 完毕，确认实施方向。

---

## Phase 1: Core 基础重构（~1 周）

### 目标
在不破坏 hub 的前提下，为 core 增加新的抽象层。

### 步骤

#### 1.1 新增 `ProviderKind` enum

```
新增: core/src/provider/kind.rs
修改: core/src/provider/mod.rs (新增 pub mod kind)
```

- 定义 `ProviderKind` enum 替代 `channel_type: i16`
- 实现 `from_channel_type()` 保持向后兼容
- 迁移 `provider_meta()` 使用 `ProviderKind`
- **不删除**旧的 `i16` 接口

#### 1.2 新增分层 Provider traits

```
修改: core/src/provider/mod.rs
```

- 新增 `Provider`, `ChatProvider`, `EmbeddingProvider`, `ResponsesProvider` trait
- 为现有 4 个 adapter 实现新 trait（delegating to 旧方法）
- 新增 `ProviderRegistry` 工厂
- **保留**旧 `ProviderAdapter` trait 和 `get_adapter()`

#### 1.3 提取共享流处理

```
新增: core/src/stream/mod.rs
新增: core/src/stream/event_stream.rs
移动: core/src/types/sse_parser.rs → core/src/stream/sse_parser.rs
      (在 types/mod.rs 保留 re-export)
```

#### 1.4 提取共享转换工具

```
新增: core/src/convert/mod.rs
新增: core/src/convert/content.rs
新增: core/src/convert/tool.rs
```

- 从 anthropic.rs 和 gemini.rs 提取重复的函数
- 旧代码调用新的共享函数

**完成标准**：
- `cargo test -p summer-ai-core` 全部通过
- 新旧 trait 并存，不影响 hub
- 共享函数有单元测试

---

## Phase 2: Hub Relay 核心域 DDD（~2 周）

### 目标
将旧的 service/relay/router 代码按 DDD 迁移到 hub 的四层结构。

### 步骤

#### 2.1 Domain 层 — Relay 领域模型

```
新增: hub/src/domain/relay/mod.rs
新增: hub/src/domain/relay/routing_policy.rs
新增: hub/src/domain/relay/channel.rs
新增: hub/src/domain/relay/token.rs
新增: hub/src/domain/relay/request.rs
```

- 定义聚合根和 Repository trait
- 定义领域事件（如 RequestCompleted, ChannelErrored）
- **纯业务逻辑，不依赖 SeaORM**

#### 2.2 Infrastructure 层 — Relay 基础设施

```
新增: hub/src/infrastructure/relay/mod.rs
新增: hub/src/infrastructure/relay/channel_router.rs
新增: hub/src/infrastructure/relay/http_client.rs
新增: hub/src/infrastructure/relay/stream.rs
新增: hub/src/infrastructure/relay/rate_limit.rs
新增: hub/src/infrastructure/relay/billing.rs
新增: hub/src/infrastructure/auth/mod.rs
新增: hub/src/infrastructure/auth/middleware.rs
新增: hub/src/infrastructure/auth/extractor.rs
```

- 从 backup 迁移代码，实现 domain 定义的 trait
- 使用 core 的新 Provider traits

#### 2.3 Application 层 — Relay 用例

```
新增: hub/src/application/relay/mod.rs
新增: hub/src/application/relay/chat.rs
新增: hub/src/application/relay/embeddings.rs
新增: hub/src/application/relay/responses.rs
新增: hub/src/application/relay/passthrough.rs
新增: hub/src/application/relay/support.rs
新增: hub/src/application/relay/tracking.rs
```

- 从 backup 迁移 service 逻辑
- 编排 domain 对象和 infrastructure 组件

#### 2.4 Interfaces 层 — HTTP 端点

```
新增: hub/src/interfaces/http/openai/mod.rs
新增: hub/src/interfaces/http/openai/chat.rs
新增: hub/src/interfaces/http/openai/embeddings.rs
新增: hub/src/interfaces/http/openai/responses.rs
新增: hub/src/interfaces/http/openai/audio.rs
新增: hub/src/interfaces/http/openai/images.rs
新增: hub/src/interfaces/http/passthrough/mod.rs
新增: hub/src/interfaces/http/passthrough/*.rs
```

- 从 backup 迁移 router 代码
- 调用 application service

#### 2.5 Plugin 注册

```
修改: hub/src/plugin.rs
```

- 注册所有 relay 路由
- 注册 auth middleware
- 注册 background job

**完成标准**：
- `cargo test -p summer-ai-hub` 通过
- AI Gateway 的 /v1/chat/completions、/v1/responses、/v1/embeddings 端点可正常工作
- 透传端点可正常工作
- 认证中间件工作正常

---

## Phase 3: Hub 管理域 DDD（~1 周）

### 步骤

#### 3.1 P1 支撑域模块

逐个模块迁移，每个模块按 DDD 四层：

```
渠道管理: channel + channel_account + channel_model_price
护栏管理: guardrail (已有样板)
告警管理: alert
运维查询: log + metrics + request + runtime
```

#### 3.2 P2 通用域模块

这些模块简化处理（可以跳过 domain 层，直接 application → infrastructure）：

```
model_config, platform_config, vendor, file_storage
multi_tenant, token, conversation
```

**完成标准**：
- 管理后台所有 API 端点恢复工作
- Background job 正常运行

---

## Phase 4: Core Adapter 深度重构（~1 周）

### 目标
Hub 已经稳定在新 trait 上后，对 core adapter 做内部优化。

### 步骤

#### 4.1 Anthropic Adapter 拆分
```
拆分: anthropic.rs (~920行) → anthropic/mod.rs + convert.rs + stream.rs
```

#### 4.2 Gemini Adapter 拆分
```
拆分: gemini.rs (~1260行) → gemini/mod.rs + convert.rs + stream.rs
```

#### 4.3 使用 StreamEventMapper

将所有 adapter 的 `parse_stream` 迁移到统一的 `mapped_chunk_stream`。

#### 4.4 删除旧 trait

确认 hub 不再使用旧 `ProviderAdapter` trait 后：
```
删除: ProviderAdapter trait
删除: get_adapter() 函数
删除: provider_scope_allowlist() 函数
```

**完成标准**：
- `cargo test` 全部通过
- 旧 trait 完全移除
- adapter 文件大小合理（<300 行/文件）

---

## Phase 5: 清理与优化（~3 天）

- [ ] 删除 `_backup` 目录（确认所有代码已迁移）
- [ ] 统一错误处理模式
- [ ] 补充缺失的测试
- [ ] 更新文档
- [ ] 清理 unused imports / dead code

---

## 二、风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| Phase 2 relay 迁移破坏 AI Gateway 功能 | 高 | 高 | 保留旧代码在 backup，逐个端点迁移+测试 |
| 新旧 trait 共存期间的混乱 | 中 | 中 | 明确标记旧 trait 为 `#[deprecated]` |
| Phase 3 管理 API 细节丢失 | 中 | 低 | backup 代码作为参考，逐行对照 |
| 重构中途需要新增功能 | 高 | 中 | 新功能直接在新架构上开发 |

## 三、核心原则提醒

1. **先让它工作，再让它正确，最后让它快** — Martin Fowler
2. **对核心域做完整 DDD，对通用域用简单 CRUD** — 不要过度工程
3. **每个 Phase 结束后 cargo test 必须通过** — 不允许累积技术债
4. **backup 是安全网** — 任何时候可以回退参考
