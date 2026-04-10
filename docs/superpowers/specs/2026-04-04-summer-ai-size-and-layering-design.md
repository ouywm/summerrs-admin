# summer-ai Size And Layering Design

## Goal

把 `crates/summer-ai` 中超过 1500 行的 Rust 文件拆分到可维护规模，并落实一个明确约束：

- `router/*` 只负责参数提取、鉴权后的入参整理、错误到 HTTP 响应的映射、最终响应包装
- 业务流程、状态机、路由决策、计费/限流协同、追踪落库、资源亲和等逻辑下沉到 `service/*` 或更底层的辅助模块

## Scope

本次范围只包含 `crates/summer-ai`。

当前超限文件：

- `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- `crates/summer-ai/hub/src/router/openai/tests/mock_upstream.rs`
- `crates/summer-ai/hub/src/router/openai.rs`
- `crates/summer-ai/core/src/provider/gemini.rs`
- `crates/summer-ai/hub/src/service/channel.rs`
- `crates/summer-ai/hub/src/service/log.rs`
- `crates/summer-ai/core/src/provider/anthropic.rs`
- `crates/summer-ai/hub/src/relay/channel_router.rs`
- `crates/summer-ai/hub/src/router/test_support.rs`

## Design Principles

### 1. Router 不写业务

路由函数只保留这些职责：

- 提取 `AiToken`、`HeaderMap`、`ClientIp`、请求体
- 调用一个 service 入口
- 将 service 返回值转换成 `Response` / `Json<T>`

路由函数不再负责：

- route plan 构建与 fallback 循环
- upstream request 构建/发送
- request/request_execution 落库
- usage 结算时机判断
- stream terminal 状态判断
- resource affinity 绑定

### 2. 按业务流而不是技术层拆文件

拆分后的文件边界要以“可独立理解的一段业务链路”为主，而不是机械按 helper/enum/struct 类型拆散。

例子：

- `service/openai_chat_relay.rs`
- `service/openai_responses_relay.rs`
- `service/openai_embeddings_relay.rs`
- `service/openai_request_tracking.rs`

这样 route、service、tracking、stream 之间的关系更清楚，也更容易测试。

### 3. 抽共享协议工具，但避免反向依赖

当前 `openai_passthrough.rs` 依赖 `router/openai.rs` 中的一批 helper。重构后要把这些 helper 提升到中立模块，避免 `service -> router` 的反向依赖。

候选共享模块：

- `service/openai_http.rs`
  - `extract_request_id`
  - `extract_upstream_request_id`
  - `insert_request_id_header`
  - `insert_upstream_request_id_header`
  - `fallback_usage`

### 4. 先大后小

优先处理对维护成本影响最大的文件：

1. `router/openai.rs`
2. `router/openai_passthrough.rs`
3. `service/channel.rs`
4. `service/log.rs`
5. `relay/channel_router.rs`
6. `router/test_support.rs`
7. `core/provider/gemini.rs`
8. `core/provider/anthropic.rs`
9. `router/openai/tests/mock_upstream.rs`

## Target Structure

### Router Layer

- `hub/src/router/openai.rs`
  - 仅保留 route 函数和极薄包装
- `hub/src/router/openai_passthrough.rs`
  - 仅保留 route 函数和 endpoint-to-spec 映射

### Service Layer

- `hub/src/service/openai_chat_relay.rs`
- `hub/src/service/openai_responses_relay.rs`
- `hub/src/service/openai_embeddings_relay.rs`
- `hub/src/service/openai_passthrough_relay.rs`
- `hub/src/service/openai_request_tracking.rs`
- `hub/src/service/openai_http.rs`

### Existing Service Splits

- `hub/src/service/channel/`
  - `crud.rs`
  - `probe.rs`
  - `health.rs`
  - `ability_sync.rs`
- `hub/src/service/log/`
  - `query.rs`
  - `dashboard.rs`
  - `mapper.rs`
- `hub/src/relay/channel_router/`
  - `cache.rs`
  - `selection.rs`
  - `scoring.rs`

### Test Support

- `hub/src/router/test_support/`
  - `fixture.rs`
  - `db_wait.rs`
  - `http_client.rs`
  - `cleanup.rs`

## Execution Strategy

### Phase 1

先重构 `router/openai.rs`：

- 提取共享 HTTP/request-id helper 到中立 service 模块
- 把 `chat/responses/embeddings` 主链路下沉到 service
- router 保留 wrapper
- 目标：`router/openai.rs <= 1500`

### Phase 2

重构 `router/openai_passthrough.rs`：

- endpoint spec、resource affinity、multipart 解析、usage settlement 分离
- 目标：`router/openai_passthrough.rs <= 1500`

### Phase 3

拆分 `channel.rs`、`log.rs`、`channel_router.rs`、`test_support.rs`

### Phase 4

拆分 provider 大文件和超长测试文件

## Testing Strategy

- 每次拆分先写一个最小 failing test 或扩充一个现有回归测试
- 每次只移动一段业务流，跑对应目标测试
- 阶段收尾时跑：
  - `cargo test -p summer-ai-hub --lib`
  - `cargo clippy -p summer-ai-hub --lib --tests -- -D warnings`

## Non-Goals

- 不在这次重构里顺手改协议行为
- 不重写计费/限流算法
- 不改数据库 schema
- 不一次性整理所有 warning 之外的历史结构问题
