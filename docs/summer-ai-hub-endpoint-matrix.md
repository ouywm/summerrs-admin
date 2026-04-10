# summer-ai-hub Endpoint Matrix

更新时间：2026-03-25

本文档用于统一三件事：

1. 官方 OpenAI API 兼容面
2. 参考项目当前实际支持面
3. `summer-ai-hub` 当前与目标实现面

---

## 1. 参考来源

### 官方文档

- Chat Completions: <https://platform.openai.com/docs/api-reference/chat>
- Responses: <https://platform.openai.com/docs/api-reference/responses>
- Embeddings: <https://platform.openai.com/docs/api-reference/embeddings>
- Images: <https://platform.openai.com/docs/api-reference/images>
- Audio: <https://platform.openai.com/docs/api-reference/audio>
- Moderations: <https://platform.openai.com/docs/api-reference/moderations>
- Files: <https://platform.openai.com/docs/api-reference/files>
- Batches: <https://platform.openai.com/docs/api-reference/batch>
- Assistants: <https://platform.openai.com/docs/api-reference/assistants>
- Threads / Runs: <https://platform.openai.com/docs/api-reference/threads>
- Vector Stores: <https://platform.openai.com/docs/api-reference/vector-stores>
- Fine-tuning: <https://platform.openai.com/docs/api-reference/fine-tuning>
- Uploads: <https://platform.openai.com/docs/api-reference/uploads>

### 本地参考项目

- `new-api`: `docs/relay/go/new-api/router/relay-router.go`
- `one-hub`: `docs/relay/go/one-hub/router/relay-router.go`
- `one-api`: `docs/relay/go/one-api/router/relay.go`
- `LiteLLM`: `docs/relay/python/litellm/litellm/proxy/public_endpoints/public_endpoints.py`
- `crabllm`: `docs/relay/rust/crabllm/crates/provider/src/provider/openai.rs`
- `hub`: `docs/relay/rust/hub/src/routes.rs`

---

## 2. 分层结论

从官方接口和参考项目对照下来，OpenAI 兼容面可以分成 4 层：

### Layer A：模型驱动的推理接口

- `/v1/chat/completions`
- `/v1/completions`
- `/v1/responses`
- `/v1/embeddings`
- `/v1/images/generations`
- `/v1/images/edits`
- `/v1/images/variations`
- `/v1/audio/transcriptions`
- `/v1/audio/translations`
- `/v1/audio/speech`
- `/v1/moderations`
- `/v1/rerank`

特点：

- 请求体里通常能拿到 `model`
- 适合走现有 `token -> ability -> channel/account -> billing -> log`

### Layer B：资源型管理接口

- `/v1/responses/{id}`
- `/v1/responses/{id}/input_items`
- `/v1/responses/{id}/cancel`
- `/v1/files*`
- `/v1/batches*`
- `/v1/assistants*`
- `/v1/threads*`
- `/v1/vector_stores*`
- `/v1/fine_tuning/jobs*`
- `/v1/uploads*`

特点：

- 很多接口没有 `model`
- 多渠道场景必须有“资源亲和”才能稳妥转发

### Layer C：发现接口

- `/v1/models`
- `/v1/models/{model}`
- `/v1/models/{model}` `DELETE`

### Layer D：特殊协议接口

- `/v1/realtime`

特点：

- 不是普通 JSON/HTTP 透传
- 需要单独的 WebSocket/Realtime 代理层

---

## 3. 参考项目覆盖矩阵

说明：

- `Y` = 已支持
- `P` = 部分支持 / RelayOnly / NotImplemented 混合
- `N` = 未看到支持

| 接口族 | new-api | one-hub | one-api | LiteLLM | crabllm | hub |
|---|---|---|---|---|---|---|
| `models` | Y | Y | Y | Y | N | N |
| `chat/completions` | Y | Y | Y | Y | Y | Y |
| `completions` | Y | Y | Y | Y | N | Y |
| `responses` | Y | Y | N | Y | N | N |
| `embeddings` | Y | Y | Y | Y | Y | Y |
| `images/generations` | Y | Y | Y | Y | Y | N |
| `images/edits` | Y | Y | P | Y | N | N |
| `images/variations` | P | Y | P | Y | N | N |
| `audio/transcriptions` | Y | Y | Y | Y | Y | N |
| `audio/translations` | Y | Y | Y | Y | N | N |
| `audio/speech` | Y | Y | Y | Y | Y | N |
| `moderations` | Y | Y | Y | Y | N | N |
| `rerank` | Y | Y | N | Y | N | N |
| `files` | P | Y | P | Y | N | N |
| `assistants` | N | Y | P | Y | N | N |
| `threads/runs` | N | Y | P | Y | N | N |
| `batches` | N | Y | N | Y | N | N |
| `vector_stores` | N | Y | N | Y | N | N |
| `fine_tuning/jobs` | N | Y | P | Y | N | N |
| `uploads` | N | N | N | Y | N | N |
| `realtime` | Y | Y | N | Y | N | N |

---

## 4. summer-ai-hub 状态矩阵

### 已完成

- `GET /v1/models`
- `GET /v1/models/{model}`
- `DELETE /v1/models/{model}`
- `POST /v1/chat/completions`
- `POST /v1/completions`
- `POST /v1/responses`
- `GET /v1/responses/{id}`
- `GET /v1/responses/{id}/input_items`
- `POST /v1/responses/{id}/cancel`
- `POST /v1/embeddings`
- `POST /v1/images/generations`
- `POST /v1/images/edits`
- `POST /v1/images/variations`
- `POST /v1/audio/transcriptions`
- `POST /v1/audio/translations`
- `POST /v1/audio/speech`
- `POST /v1/moderations`
- `POST /v1/rerank`
- `GET/POST /v1/files`
- `GET/DELETE /v1/files/{id}`
- `GET /v1/files/{id}/content`
- `GET/POST /v1/batches`
- `GET /v1/batches/{id}`
- `POST /v1/batches/{id}/cancel`
- `GET/POST /v1/assistants`
- `GET/POST/DELETE /v1/assistants/{id}`
- `POST /v1/threads`
- `GET/POST/DELETE /v1/threads/{id}`
- `POST /v1/threads/{id}/messages`
- `GET/POST /v1/threads/{id}/messages/{message_id}`
- `GET /v1/threads/{id}/messages`
- `POST /v1/threads/{id}/runs`
- `POST /v1/threads/runs`
- `GET/POST /v1/threads/{id}/runs/{run_id}`
- `GET /v1/threads/{id}/runs`
- `POST /v1/threads/{id}/runs/{run_id}/submit_tool_outputs`
- `POST /v1/threads/{id}/runs/{run_id}/cancel`
- `GET /v1/threads/{id}/runs/{run_id}/steps`
- `GET /v1/threads/{id}/runs/{run_id}/steps/{step_id}`
- `GET/POST /v1/vector_stores`
- `GET/POST/DELETE /v1/vector_stores/{id}`
- `POST /v1/vector_stores/{id}/search`
- `GET/POST /v1/vector_stores/{id}/files`
- `GET/DELETE /v1/vector_stores/{id}/files/{file_id}`
- `GET/POST /v1/vector_stores/{id}/file_batches`
- `GET /v1/vector_stores/{id}/file_batches/{batch_id}`
- `POST /v1/vector_stores/{id}/file_batches/{batch_id}/cancel`
- `GET/POST /v1/fine_tuning/jobs`
- `GET /v1/fine_tuning/jobs/{id}`
- `POST /v1/fine_tuning/jobs/{id}/cancel`
- `GET /v1/fine_tuning/jobs/{id}/events`
- `GET /v1/fine_tuning/jobs/{id}/checkpoints`
- `POST /v1/uploads`
- `GET /v1/uploads/{id}`
- `POST /v1/uploads/{id}/parts`
- `POST /v1/uploads/{id}/complete`
- `POST /v1/uploads/{id}/cancel`

### 本轮目标

#### A. 高价值模型驱动接口

- 补齐多 provider 在这些接口上的真实运行时回归
- 继续校准 `/v1/models` 可见面与真实配置面
- 为需要计费的接口补齐完整 usage / settlement 语义

#### B. 资源型透传接口

- 继续补资源链的 route-level 回归用例
- 继续补 `runs` 之外资源 POST 的计费语义边界
- 继续增强资源响应里的多 ID 绑定覆盖面

### 暂缓

- `GET /v1/realtime`

原因：

- 该接口属于 Realtime/WebSocket 代理，不是普通 HTTP JSON 透传
- 需要单独的升级握手与双向流桥接层

---

## 5. 实现策略

### 5.1 模型驱动接口

统一走现有主链路：

1. `AiAuthLayer` 验证 token
2. handler 提取 `ClientIp`
3. 根据 `model + endpoint_scope` 选路
4. 预扣配额
5. 转发上游
6. 成功后结算 / 失败回滚
7. 异步写 AI usage log

### 5.2 资源型接口

增加 `ResourceAffinityService`：

1. 创建型接口成功后，把返回的资源 `id` 绑定到选中的 `channel/account`
2. 后续 `GET/POST/DELETE` 命中同一资源时，优先按资源亲和路由
3. 若查不到资源亲和，则回退到默认渠道选择

### 5.3 默认渠道选择

给 `ChannelRouter` 增加“无模型默认路由”能力：

1. 先看当前 `group + endpoint_scope`
2. 若没有配置，再回退到该 `group` 下任意已启用可调度渠道

---

## 6. 开发顺序

### Step 1

- 补文档
- 补 `ResourceAffinityService`
- 补 `ChannelRouter::select_default_channel`

### Step 2

- 补 `completions`
- 补 `images/*`
- 补 `audio/*`
- 补 `moderations`
- 补 `rerank`
- 补 `models/{model}`

### Step 3

- 补 `responses/{id}*`
- 补 `files*`
- 补 `batches*`
- 补 `assistants*`
- 补 `threads*`
- 补 `vector_stores*`
- 补 `fine_tuning/jobs*`
- 补 `uploads*`

### Step 4

- 增补测试
- 增补 curl 回归样例
- 视情况再做 Realtime

---

## 7. 当前实现边界说明

这一轮的目标是先把“OpenAI 兼容 HTTP 面”铺全，并让单渠道/常规多渠道场景先跑起来。

后续仍值得继续补强的点：

- 资源型接口更精细的计费一致性
- Realtime WebSocket 代理
- 官方 `OpenAI-Beta` 相关版本头的专项兼容测试
- 更细粒度的 endpoint_scope 配置治理
