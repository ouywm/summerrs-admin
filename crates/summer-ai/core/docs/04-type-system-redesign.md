# 04 — 类型系统重构

## 一、当前类型分布

```
summer-ai-core/src/types/     — OpenAI 兼容 API 类型（请求/响应 DTO）
summer-ai-model/src/entity/   — SeaORM 数据库实体（~70 张表）
summer-ai-model/src/dto/      — 管理后台 API 的 DTO
summer-ai-model/src/vo/       — 值对象 (view object)
```

### 各模块类型定位

| 模块 | 类型 | 用途 | 面向 |
|------|------|------|------|
| `core/types/chat.rs` | `ChatCompletionRequest/Response` | OpenAI Chat API | AI 调用者 |
| `core/types/responses.rs` | `ResponsesRequest/Response` | OpenAI Responses API | AI 调用者 |
| `core/types/embedding.rs` | `EmbeddingRequest/Response` | OpenAI Embedding API | AI 调用者 |
| `core/types/error.rs` | `OpenAiErrorResponse` | 标准化错误返回 | AI 调用者 |
| `core/types/common.rs` | `Message, Usage, Tool` | 跨 endpoint 共用 | 内部 |
| `core/types/sse_parser.rs` | `SseParser` | SSE 流解析 | 内部 |
| `model/entity/*` | SeaORM `Model` + `ActiveModel` | 数据库 CRUD | 内部 |
| `model/dto/*` | 管理 API DTO | 管理后台请求/响应 | 管理员 |
| `model/vo/*` | 值对象 | 组合查询/展示 | 管理员 |

## 二、类型设计决策

### 决策：保持 OpenAI-compatible 作为核心类型

**原因**：
1. summer-ai 本质上是一个 **OpenAI-compatible AI Gateway**
2. 绝大多数调用者（前端/SDK）期望 OpenAI 格式
3. 创建中间类型（`SummerChatRequest` → `OpenAiChatRequest`）是不必要的间接层
4. Anthropic/Gemini adapter 的工作就是「翻译成 OpenAI 格式」

**不做的事**：
- ❌ 不创建 `SummerMessage`、`SummerUsage` 等自有类型
- ❌ 不在 core 层做 OpenAI → Summer → Provider 的双重转换
- ✅ core/types 就是 OpenAI 标准类型
- ✅ Provider adapter 负责 Native → OpenAI 转换

### 需要改进的地方

#### 改进 1: SSE Parser 归入 stream 模块

```
# 之前
types/sse_parser.rs    — 放在 types 下不合适，它不是类型定义

# 之后
stream/sse_parser.rs   — 放在 stream 模块下，与流处理相关
```

#### 改进 2: 补全缺失的 Endpoint 类型

```rust
// 新增 types/rerank.rs
pub struct RerankRequest {
    pub model: String,
    pub query: String,
    pub documents: Vec<String>,
    pub top_n: Option<i32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub struct RerankResponse {
    pub results: Vec<RerankResult>,
    pub usage: Usage,
}
```

#### 改进 3: 强化 common.rs 中的类型

```rust
// 当前 Message.role 是 String，可以考虑 enum
// 但为了 OpenAI 兼容性和未来新 role 的扩展，保持 String 是正确的

// 当前 Message.content 是 serde_json::Value，这是正确的
// 因为 OpenAI content 可以是 string、array、null

// 改进：增加辅助方法
impl Message {
    /// 提取纯文本内容
    pub fn text_content(&self) -> Option<&str> {
        self.content.as_str()
    }

    /// 判断是否为 system 消息
    pub fn is_system(&self) -> bool {
        self.role == "system"
    }

    /// 判断是否为 user 消息
    pub fn is_user(&self) -> bool {
        self.role == "user"
    }
}
```

## 三、model crate 在 DDD 中的定位

### 现状

```
summer-ai-model/
├── entity/         # SeaORM 实体 (~70个)
│   ├── _entity/    # 自动生成的 SeaORM entity 代码
│   └── *.rs        # 手动增强（impl 块、关联查询等）
├── dto/            # 管理后台 DTO (~15个)
└── vo/             # 值对象 (~15个)
```

### DDD 视角的重新定位

| 模块 | DDD 角色 | 说明 |
|------|---------|------|
| `entity/_entity/` | **Infrastructure 层** — 持久化模型 | 纯 ORM 映射，自动生成 |
| `entity/*.rs` | **Infrastructure 层** — 仓储辅助 | 查询 scope、关联查询 |
| `dto/` | **Application 层** — 数据传输对象 | 管理 API 的输入/输出 |
| `vo/` | **Application 层** — 视图对象 | 组合查询结果 |

### 建议：model crate 保持现状不动

**原因**：
1. `model` 是一个共享基础设施 crate，被 `hub` 依赖
2. 它的 entity 是 SeaORM 持久化层，属于 Infrastructure
3. 它的 dto/vo 是管理后台的应用层类型
4. 在 hub 的 DDD 重构中，domain 层会定义自己的 Aggregate，与 model 的 entity 分离（如 guardrail_config 示例已展示）
5. 移动 model 的代码收益不大，但破坏面很广

```
# 依赖关系（保持不变）
hub::domain       → 不依赖 model
hub::application  → 可以引用 model/dto（管理模块的 DTO）
hub::infrastructure → 依赖 model/entity（ORM 查询）
hub::interfaces   → 可以引用 model/dto（管理 API）
```

## 四、core types 与 hub domain types 的关系

```
                    AI Gateway 请求处理流
                    =====================

外部请求 (OpenAI 格式)
    │
    ▼
core::types::ChatCompletionRequest    ← core 定义的 API 类型
    │
    ▼ (hub interfaces 层接收)
hub::interfaces → hub::application    ← hub 的 Relay 用例
    │
    ▼ (hub infrastructure 层执行)
core::provider::ChatProvider          ← core 的 adapter 执行上游调用
    │
    ▼ (返回)
core::types::ChatCompletionResponse   ← core 定义的 API 类型
    │
    ▼ (hub 做后处理：计费、日志、缓存)
返回给调用者


                    管理后台 CRUD 流
                    ================

管理请求 (JSON)
    │
    ▼
model::dto::ChannelCreateRequest      ← model 定义的管理 DTO
    │
    ▼ (hub interfaces 层接收)
hub::interfaces → hub::application
    │
    ▼ (hub domain 层验证)
hub::domain::ChannelAggregate         ← hub domain 定义的聚合根
    │
    ▼ (hub infrastructure 层持久化)
model::entity::channel::ActiveModel   ← model 的 ORM 实体
    │
    ▼ (写入数据库)
```

## 五、类型转换链总结

### AI Gateway 请求
```
HTTP JSON → core::types::ChatCompletionRequest → provider adapter → upstream HTTP
         → core::types::ChatCompletionResponse → hub 后处理 → HTTP JSON
```

### 管理后台 CRUD
```
HTTP JSON → model::dto::XxxRequest → domain::XxxAggregate → model::entity::XxxActiveModel → DB
DB → model::entity::XxxModel → domain::XxxAggregate → application::XxxDto → interfaces::XxxResponse → HTTP JSON
```

### 类型所有权

| 类型 | 所属 crate | 谁创建 | 谁消费 |
|------|-----------|--------|--------|
| `ChatCompletionRequest` | core | interfaces (反序列化) | provider adapter |
| `ChatCompletionResponse` | core | provider adapter | interfaces (序列化) |
| `OpenAiErrorResponse` | core | hub (错误处理) | interfaces (序列化) |
| `ChannelAggregate` | hub/domain | hub/infrastructure (from ORM) | hub/application |
| `ChannelCreateDto` | model/dto | interfaces (反序列化) | hub/application |
| `channel::Model` | model/entity | SeaORM (from DB row) | hub/infrastructure |
