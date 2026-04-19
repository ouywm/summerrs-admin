# 多入口协议转换规格（CONVERSION_SPEC）

> **目的**：定义 Claude / Gemini / OpenAIResponses 三种客户端入口协议 ↔ **我们的 canonical（OpenAI Chat Completions 扁平结构）** 的字段映射、content block 翻译、流事件状态机。
>
> **来源**：本文档基于 NewAPI `service/convert.go`（1007 行）+ `service/openaicompat/chat_to_responses.go`（402 行）整理，**几乎照搬 NewAPI 的映射规则**（已踩过的坑都继承）。
>
> **ARCHITECTURE.md §4 引用本文档**。Rust `IngressConverter` / `EgressConverter` 实现时**照表写代码**。

更新日期：2026-04-19

---

## 0. 总览

### 0.1 三种入口协议

| 入口 | HTTP 路径 | 客户端 SDK 示例 | 典型场景 |
|---|---|---|---|
| **OpenAI Chat**（识别为 canonical） | `POST /v1/chat/completions` | openai-python | 主流 |
| **Claude Messages** | `POST /v1/messages` | anthropic-sdk / Claude Code CLI | Anthropic 生态 |
| **Gemini GenerateContent** | `POST /v1beta/models/{model}:generateContent` `:streamGenerateContent` | google-generativeai | Google 生态 |
| **OpenAI Responses** | `POST /v1/responses` | openai-python（新 API） | GPT-5 / o1 思维链 |

### 0.2 双向转换需求

```
客户端请求（Claude/Gemini/Responses）
    ↓ IngressConverter::to_canonical
canonical ChatRequest (OpenAI-flat)
    ↓ AdapterDispatcher::build_chat_request
上游 wire (OpenAI/Anthropic/Gemini/...)
    ↓ Adapter::parse_chat_response(_stream_event)
canonical ChatResponse / ChatStreamEvent (OpenAI-flat)
    ↓ EgressConverter::from_canonical(_stream_event)
客户端响应（Claude/Gemini/Responses）
```

**关键**：
- `canonical` 就是 OpenAI Chat Completions 扁平格式（我们已在 `core/src/types/openai/chat.rs` 对齐官方）
- Ingress 方向：把入口协议**向下翻译**到 canonical
- Egress 方向：把 canonical **向上翻译**回入口协议
- Adapter 永远只见 canonical

### 0.3 我们 canonical 类型位置

```rust
core/src/types/common/
├── message.rs      // ChatMessage + Role + ContentPart + MessageContent
├── tool.rs         // Tool + ToolCall + ToolChoice
├── usage.rs        // Usage + FinishReason + PromptTokensDetails
└── stream_event.rs // ChatStreamEvent + StreamEnd + ToolCallDelta

core/src/types/openai/
├── chat.rs         // ChatRequest + ChatResponse (扁平 OpenAI wire)
└── ...

core/src/types/ingress_wire/    // 入口协议的 wire 类型
├── claude.rs       // ClaudeMessagesRequest + ClaudeResponse + ClaudeStreamEvent
├── gemini.rs       // GeminiChatRequest + GeminiChatResponse
└── openai_resp.rs  // OpenAIResponsesRequest + OpenAIResponsesResponse + OpenAIResponsesStreamEvent
```

---

## 1. Claude ↔ canonical

### 1.1 请求字段映射（Claude Messages → ChatRequest）

| Claude 字段 | canonical 字段 | 规则 |
|---|---|---|
| `model` | `model` | 原样 |
| `max_tokens` | `max_tokens` | 原样 |
| `temperature` | `temperature` | 原样 |
| `top_p` | `top_p` | 原样 |
| `top_k` | `top_k` | 原样（canonical 也支持 top_k） |
| `stream` | `stream` | 原样 |
| `stop_sequences: Vec<String>` | `stop: serde_json::Value` | 1 个 → 字符串；>1 个 → 数组 |
| `system: String / Vec<ContentBlock>` | `messages[0] = {role:"system",content:...}` | 插入最前（见 1.1.1） |
| `messages: Vec<ClaudeMessage>` | `messages: Vec<ChatMessage>` | 每条见 1.2 / 1.3 |
| `tools: Vec<ClaudeTool>` | `tools: Vec<Tool>` | `input_schema` → `parameters` |
| `thinking: ThinkingConfig` | **上游特殊处理** | 见 1.1.2 |
| `tool_choice` | `tool_choice` | 结构相同 |
| `metadata.user_id` | `user` | string 传递 |

#### 1.1.1 system 字段的两种形态

Claude 的 `system` 可以是字符串**或**多块（Content Block 数组），canonical 统一成 `system role message`：

```
Claude: {"system": "You are helpful"}
→ canonical: messages[0] = {role:"system", content:"You are helpful"}

Claude: {"system": [{"type":"text","text":"A"}, {"type":"text","text":"B","cache_control":...}]}
→ canonical（默认合并）: messages[0] = {role:"system", content:"A\nB"}
→ canonical（上游支持 cache_control 时）: messages[0] = {role:"system", content:[{type:"text",text:"A"},{type:"text",text:"B",cache_control:...}]}
```

上游支持 `cache_control` 的清单：Anthropic native、OpenRouter (anthropic/*)。其他上游丢弃 `cache_control`。

#### 1.1.2 thinking 字段的两种处理

```
Claude: {"thinking": {"type": "enabled", "budget_tokens": 1024}}
```

- **上游 = Anthropic native**：原样透传（Anthropic 自家认识）
- **上游 = OpenRouter (anthropic/*)**：转成 `reasoning: {enabled: true, max_tokens: 1024}`（OpenRouter 方言）
- **上游 = 其他**：模型名加后缀 `-thinking`（NewAPI 的约定），上游自己识别

```
Claude: {"thinking": {"type": "adaptive"}}
→ OpenRouter: {"reasoning": {"enabled": true}}
→ 其他: 模型名加 -thinking 后缀
```

### 1.2 消息 content 映射（字符串 or 多块）

Claude 消息有两种 content：

```
{role:"user", content:"hi"}                                                # 字符串
{role:"user", content:[{type:"text",text:"hi"}, {type:"image",source:...}]} # 多块
```

**映射规则**：

| Claude block `type` | canonical 处理 |
|---|---|
| `text` / `input_text` | `ContentPart::Text { text }`（保留 `cache_control` 字段） |
| `image` | `ContentPart::ImageUrl { image_url: { url: "data:{mime};base64,{data}" } }` |
| `tool_use` | **不放进 content**，提升到 `ChatMessage.tool_calls[i]`（见 1.3） |
| `tool_result` | **不放进当前消息**，生成一条**新** `role:"tool"` 消息追加（见 1.3） |

**字符串形态的保留**：如果 content 全部是单条 `text` block，**canonical 仍用字符串**形态（不变成单元素数组）——OpenAI 官方这两种形态都接受，保持最简。

### 1.3 工具调用 / 工具响应映射

Claude 的 `tool_use` 和 `tool_result` 是 content blocks，在 canonical 里是**独立的字段/消息**。

#### 1.3.1 assistant 发起 tool call

```
Claude:
{
  "role": "assistant",
  "content": [
    {"type":"text", "text":"let me check"},
    {"type":"tool_use", "id":"tu_1", "name":"weather", "input":{"city":"NYC"}}
  ]
}

→ canonical:
{
  "role": "assistant",
  "content": "let me check",                  // 只保留 text blocks
  "tool_calls": [
    {
      "id": "tu_1",
      "type": "function",
      "function": {
        "name": "weather",
        "arguments": "{\"city\":\"NYC\"}"     // Claude 的 input(object) → arguments(string)
      }
    }
  ]
}
```

**规则**：
- `tool_use.id` → `tool_calls[i].id`
- `tool_use.name` → `tool_calls[i].function.name`
- `tool_use.input` (object) → `tool_calls[i].function.arguments` (**string**, JSON-serialized)
- 若消息有 `tool_calls`，其 `content` 只保留 text blocks（不再带 media blocks）

#### 1.3.2 user 返回 tool result

```
Claude:
{
  "role": "user",
  "content": [
    {"type":"tool_result", "tool_use_id":"tu_1", "content":"72F"},
    {"type":"text", "text":"what's next"}
  ]
}

→ canonical（拆成 2 条消息）:
[
  {
    "role": "tool",
    "tool_call_id": "tu_1",
    "name": "weather",                   // 由 tool_use_id 反查历史得到
    "content": "72F"
  },
  {
    "role": "user",
    "content": "what's next"
  }
]
```

**规则**：
- `tool_result` **单独提取成一条 `role:"tool"` 消息**，塞在当前 user 消息**之前**
- `tool_use_id` → `tool_call_id`
- `name` 字段：Claude 的 `tool_result` **不带** tool 名，需反查历史 tool_use 得到（维护一个 `tool_use_id → name` map）
- `tool_result.content` 可以是字符串或多块：
  - 字符串 → 直接用
  - 多块（text + image） → **JSON-stringify 整个 block 数组**作为 `content`

### 1.4 响应字段映射（canonical ChatResponse → ClaudeResponse）

非流式场景。canonical 是 OpenAI 格式的 ChatResponse，要转回 Claude 的 `{type:"message", content:[...], ...}` 结构。

| canonical 字段 | Claude 字段 | 规则 |
|---|---|---|
| `id` | `id` | 原样 |
| `model` | `model` | 原样 |
| `choices[0].message.content` | `content[*]` | 字符串 → 单个 `{type:"text", text:...}` block |
| `choices[0].message.tool_calls[]` | `content[*]` | 每个 tool_call → 一个 `{type:"tool_use", ...}` block |
| `choices[0].finish_reason` | `stop_reason` | 见 1.6 |
| `usage.prompt_tokens` | `usage.input_tokens` | |
| `usage.completion_tokens` | `usage.output_tokens` | |
| `usage.prompt_tokens_details.cached_tokens` | `usage.cache_read_input_tokens` | |
| `usage.prompt_tokens_details.cached_creation_tokens` | `usage.cache_creation_input_tokens` | |
| `usage.claude_cache_creation_5m_tokens` | `usage.cache_creation.ephemeral_5m_input_tokens` | 仅 Anthropic 上游返 |
| `usage.claude_cache_creation_1h_tokens` | `usage.cache_creation.ephemeral_1h_input_tokens` | 同上 |

### 1.5 tool_use 的 arguments 反向

canonical `tool_calls[].function.arguments` 是**字符串**（JSON 序列化），Claude 的 `tool_use.input` 是**对象**。反向转换：

```
canonical: arguments = "{\"city\":\"NYC\"}"
→ Claude: try parse to object; 失败则当字符串
Claude: input = {"city":"NYC"}
```

### 1.6 finish_reason ↔ stop_reason

对照表（NewAPI `reasonmap` 包）：

| canonical `finish_reason` | Claude `stop_reason` |
|---|---|
| `stop` | `end_turn` |
| `length` | `max_tokens` |
| `tool_calls` | `tool_use` |
| `content_filter` | `stop_sequence`（兜底） |
| `function_call`（legacy） | `tool_use` |
| 空/未知 | `end_turn`（Claude 要求非空） |

---

### 1.7 流事件重组（canonical ChatStreamEvent → Claude SSE）

**这是最复杂的一块**——Claude SSE 有 **6 种事件**，canonical 就是一连串 `ChatStreamEvent::Chunk(choice.delta)`，需要一个**状态机**把 delta 流重组成 Claude 的 block 状态机。

#### 1.7.1 Claude SSE 事件规格

```
event: message_start
data: {"type":"message_start","message":{"id":...,"model":...,"role":"assistant","content":[],"usage":{"input_tokens":N,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":M}}

event: message_stop
data: {"type":"message_stop"}
```

规则：**`content_block_start` → `content_block_delta*` → `content_block_stop`** 每个 index 一轮。不同 block 类型：`text` / `thinking` / `tool_use`。

#### 1.7.2 状态机需要维护的 state

```rust
pub struct ClaudeStreamConvertState {
    /// 客户端收到的事件总数（1 表示还没发 message_start）
    pub send_response_count: u32,

    /// 当前 block 类型
    pub last_message_type: LastMessageType,  // None | Text | Thinking | Tools

    /// 当前 block 的 content_block index（text/thinking 最多一个；tool 可并行多个）
    pub index: i32,

    /// tool_use 并发时的起点 index
    pub tool_call_base_index: i32,

    /// tool_use 并发时的最大 offset
    pub tool_call_max_index_offset: i32,

    /// 上游 finish_reason（停止时写入）
    pub finish_reason: String,

    /// 累积的 usage（上游可能先 finish_reason 再送 usage 只块）
    pub usage: Option<Usage>,

    /// 是否已发 message_stop
    pub done: bool,
}

pub enum LastMessageType {
    None,
    Text,
    Thinking,
    Tools,
}
```

#### 1.7.3 状态机逻辑

**首次调用**（`send_response_count == 1`）：
1. 发 `message_start`
2. 若首块就有 tool_call：发 `content_block_start{tool_use}` + 可能的 `content_block_delta{input_json_delta}`
3. 若首块有 `reasoning_content`：`stopOpenBlocksAndAdvance` + `content_block_start{thinking}` + `content_block_delta{thinking_delta}`
4. 若首块有 `content`：`stopOpenBlocksAndAdvance` + `content_block_start{text}` + `content_block_delta{text_delta}`
5. 若首块带 `finish_reason`：`stopOpenBlocks` + `message_delta` + `message_stop` + `done=true`

**后续调用**：
1. `len(choices) == 0 && usage != nil`（usage-only 尾块）：`stopOpenBlocks` + `message_delta` + `message_stop`
2. `finish_reason != None && usage == None`：延后（某些上游先 finish 再送 usage），返空
3. `delta.tool_calls.len() > 0`：
   - 若 LastType != Tools，`stopOpenBlocksAndAdvance`，记 `tool_call_base_index = index`
   - LastType = Tools
   - 对每个 tool_call：
     - 计算 `block_index = tool_call_base_index + toolCall.index (or i)`
     - 若有 `function.name` 非空：发 `content_block_start{tool_use}`
     - 若有 `function.arguments` 非空：发 `content_block_delta{input_json_delta}`
4. `delta.reasoning_content != None`：
   - 若 LastType != Thinking，`stopOpenBlocksAndAdvance` + `content_block_start{thinking}`，LastType = Thinking
   - 发 `content_block_delta{thinking_delta}`
5. `delta.content != None`：
   - 若 LastType != Text，`stopOpenBlocksAndAdvance` + `content_block_start{text}`，LastType = Text
   - 发 `content_block_delta{text_delta}`
6. 若此 chunk `finish_reason != None`：`stopOpenBlocks` + `message_delta` + `message_stop` + `done=true`

**`stopOpenBlocks` 子程序**：
- LastType = Text / Thinking：发 1 条 `content_block_stop{index}`
- LastType = Tools：为 `[base .. base+maxOffset]` 每个 idx 发一条 `content_block_stop`

**`stopOpenBlocksAndAdvance`**：先 `stopOpenBlocks`，再 `index++`（或 tool 跳到 `base+maxOffset+1`），LastType = None

#### 1.7.4 usage 累积

- 每收到 OpenAI chunk，若 `usage != None`，**更新** state.usage（不累加，取最后的）
- 首块 `message_start` 用 `estimated_prompt_tokens`（路由时由 tokenizer 估算）填 input_tokens
- 结束时 `message_delta.usage.output_tokens` 用 state.usage.completion_tokens

#### 1.7.5 Cache creation 5m/1h 归一化

```
NormalizeCacheCreationSplit(total, 5m, 1h):
  remainder = max(total - 5m - 1h, 0)
  return (5m + remainder, 1h)
```

---

## 2. Gemini ↔ canonical

### 2.1 请求字段映射（GeminiChatRequest → ChatRequest）

| Gemini 字段 | canonical 字段 | 规则 |
|---|---|---|
| `contents: Vec<Content>` | `messages: Vec<ChatMessage>` | 每条见 2.2 |
| `systemInstructions.parts[*].text` | `messages[0] = {role:"system", content:拼接文本}` | 所有 text parts 以 `\n` 连接 |
| `generationConfig.temperature` | `temperature` | 原样 |
| `generationConfig.topP` | `top_p` | 原样（> 0 才设） |
| `generationConfig.topK` | `top_k` | `*int64` → `Option<i32>` |
| `generationConfig.maxOutputTokens` | `max_tokens` | > 0 才设 |
| `generationConfig.stopSequences` | `stop` | Gemini 最多 5 个，canonical 最多 4 个，**截取前 4** |
| `generationConfig.candidateCount` | `n` | > 0 才设 |
| `tools[*].functionDeclarations` | `tools` | 见 2.3 |
| `toolConfig.functionCallingConfig` | `tool_choice` | 见 2.4 |
| `model`（URL 路径里） | `model` | 从 URL 取 |
| `stream`（由路径 `:streamGenerateContent` 推断） | `stream` | path 判定 |

### 2.2 Role 映射

```
Gemini    → canonical
"user"    → "user"
"model"   → "assistant"
"function"→ "function" (legacy) / "tool" (新)
其他      → "user"（兜底）
```

### 2.3 Parts（content）映射

Gemini 的 content 是 `parts: Vec<Part>`，canonical 是 `content: MessageContent`。

| Gemini part 字段 | canonical 处理 |
|---|---|
| `text: String` | `ContentPart::Text { text }` |
| `inlineData: {mimeType, data}` | `ContentPart::ImageUrl { image_url: { url: "data:{mime};base64,{data}", mime_type } }` |
| `fileData: {mimeType, fileUri}` | `ContentPart::ImageUrl { image_url: { url: fileUri, mime_type } }` |
| `functionCall: {name, args}` | **不放 content**，提升到 `tool_calls`（见 2.5） |
| `functionResponse: {name, response}` | **不放当前消息**，生成 `role:"tool"` 消息追加（见 2.5） |

**合并规则**：
- 若只有 1 个 text part：`content` 直接是字符串
- 若多个或含媒体：`content` 是 parts 数组
- 全部是 tool_call / tool_response：content 留空，tool_calls 另存

### 2.4 functionDeclarations → tools

```
Gemini:
{
  "tools": [{
    "functionDeclarations": [
      {"name":"weather", "description":"...", "parameters":{...}}
    ]
  }]
}

→ canonical:
{
  "tools": [
    {
      "type": "function",
      "function": {"name":"weather", "description":"...", "parameters":{...}}
    }
  ]
}
```

### 2.5 工具调用 / 响应映射

#### 2.5.1 assistant 发起（Gemini model role）

```
Gemini:
{
  "role": "model",
  "parts": [
    {"text":"let me check"},
    {"functionCall": {"name":"weather", "args":{"city":"NYC"}}}
  ]
}

→ canonical:
{
  "role": "assistant",
  "content": "let me check",
  "tool_calls": [
    {
      "id": "call_1",               // 自动生成（Gemini 没有 call_id 概念）
      "type": "function",
      "function": {
        "name": "weather",
        "arguments": "{\"city\":\"NYC\"}"
      }
    }
  ]
}
```

**call_id 生成**：NewAPI 用 `call_${len(toolCalls)+1}` 简单自增。我们也照这个。

#### 2.5.2 user 返回 tool response（Gemini 在同一条 user content 里）

```
Gemini:
{
  "role": "user",
  "parts": [
    {"functionResponse": {"name":"weather", "response":{"temp":"72F"}}},
    {"text":"what's next"}
  ]
}

→ canonical（拆成 2 条）:
[
  {
    "role": "tool",
    "tool_call_id": "call_1",       // 对应生成的 id
    "content": "{\"temp\":\"72F\"}"  // response 序列化成 JSON 字符串
  },
  {
    "role": "user",
    "content": "what's next"
  }
]
```

### 2.6 响应字段映射（canonical → GeminiChatResponse）

非流式。

| canonical 字段 | Gemini 字段 | 规则 |
|---|---|---|
| `choices[]` | `candidates[]` | 一对一 |
| `choices[i].index` | `candidates[i].index` | |
| `choices[i].message.content` | `candidates[i].content.parts[0].text` | 单文本 part |
| `choices[i].message.tool_calls[]` | `candidates[i].content.parts[].functionCall` | 每个 tool_call 一个 part，`arguments` 字符串 → **JSON 解析回对象** |
| `choices[i].finish_reason` | `candidates[i].finish_reason` | 见 2.7 |
| `usage.prompt_tokens` | `usageMetadata.promptTokenCount` | |
| `usage.completion_tokens` | `usageMetadata.candidatesTokenCount` | |
| `usage.total_tokens` | `usageMetadata.totalTokenCount` | |

**content.role 写死**：`"model"`（Gemini 的 assistant）

**safetyRatings**：返空数组（canonical 没有此信息）

### 2.7 finish_reason 映射

```
canonical → Gemini
"stop"           → "STOP"
"length"         → "MAX_TOKENS"
"content_filter" → "SAFETY"
"tool_calls"     → "STOP"       (!! 不是 "TOOL_CALLS"，NewAPI 就是这么映)
其他             → "STOP"
```

### 2.8 流响应映射

Gemini 流没有"SSE 事件类型"概念，每次就返一个**完整的 GeminiChatResponse JSON**（像"轮询"）。规则：

1. canonical stream event 内容是空的（delta 没有 content 也没有 tool_calls 也没有 finish_reason）→ **跳过**
2. 否则构造一个 `GeminiChatResponse`，`candidates[]` 对应 canonical choices
3. `delta.content` → `candidates[0].content.parts[0].text`
4. `delta.tool_calls` → `candidates[0].content.parts[*].functionCall`（参数反 JSON 解析）
5. `finish_reason` → `candidates[0].finishReason`
6. `usage`（仅末块）→ `usageMetadata`（前面的块可以用估算的 prompt tokens）

---

## 3. OpenAIResponses ↔ canonical

### 3.1 Responses API 简介

OpenAI 的 `/v1/responses` 是 ChatCompletions 的**新后继**，差异：

| 维度 | Chat Completions | Responses |
|---|---|---|
| 消息字段 | `messages` | `input`（结构不同） |
| 系统提示 | `messages` 里的 `role:"system"` | 顶层 `instructions` 字段 |
| tool 调用 | `tool_calls[]` 数组 | `function_call` item / `function_call_output` item |
| 响应格式 | `response_format` | `text.format`（扁平化） |
| max tokens | `max_tokens` / `max_completion_tokens` | `max_output_tokens` |
| reasoning | `reasoning_effort: "low/medium/high"` | `reasoning: {effort, summary}` |
| 其他 | `store`, `metadata`, `user`, `temperature`, `top_p`, `tools`, `tool_choice`, `parallel_tool_calls` | 多数相同 |

**约束**：`n > 1` 在 Responses 里**不支持**，直接返错。

### 3.2 Chat → Responses（IngressConverter，客户端走 Responses API 入口）

客户端用 `POST /v1/responses` 请求，我们要把 Responses wire → canonical (OpenAI Chat)。**反过来**：canonical (Chat) → Responses wire 发给上游（当上游是 Responses API 时）。

NewAPI 文件 `chat_to_responses.go` 里只做了 **Chat → Responses** 方向（因为 canonical 是 Chat）。Responses → Chat 是镜像（反向处理 items → messages）。

### 3.3 canonical → Responses 的 `input` 构造（messages → input items）

Chat 的每条消息映射到 Responses 的 1 或多个 **input items**：

| Chat 消息 `role` | Responses `input` item(s) | 备注 |
|---|---|---|
| `system` / `developer` | **不进 input**，合并成顶层 `instructions` 字符串 | 多条用 `\n\n` 连接 |
| `user` | 1 个 item `{role:"user", content:...}` | content 见 3.4 |
| `assistant`（只有 text） | 1 个 item `{role:"assistant", content:...}` | |
| `assistant`（含 tool_calls） | 1 个 item + 每个 tool_call 额外 1 个 `function_call` item | 见 3.5 |
| `tool` / `function` | 1 个 item `{type:"function_call_output", call_id, output}` | 见 3.6 |

### 3.4 content 扁平化（parts 的 type 重命名）

Chat 的 `ContentPart` → Responses 的 content parts（type 名字变了）：

| Chat type | Responses type |
|---|---|
| `text`（user 消息） | `input_text` |
| `text`（assistant 消息） | `output_text` |
| `image_url` | `input_image`（字段 `image_url` 从 object/string 归一化成字符串 URL） |
| `input_audio` | `input_audio`（原样） |
| `file` | `input_file` |
| `video_url` | `input_video` |
| 其他未知 | `{type: part.type}` 保留 type 字段 |

**特例**：若 content 只是单字符串（`IsStringContent`），直接塞 `item.content = "..."`，不变成数组。

### 3.5 assistant tool_calls 的拆分

```
Chat:
{
  "role":"assistant",
  "content":"let me check",
  "tool_calls":[
    {"id":"tc_1","type":"function","function":{"name":"weather","arguments":"{\"city\":\"NYC\"}"}}
  ]
}

→ Responses input[]:
[
  {"role":"assistant","content":"let me check"},
  {"type":"function_call","call_id":"tc_1","name":"weather","arguments":"{\"city\":\"NYC\"}"}
]
```

**规则**：
- tool_calls 不嵌在 assistant message 里，拍平成独立的 `function_call` items
- 跳过 `id` 为空、`type != "function"`、`function.name` 为空的 tool_call
- `arguments` **保持字符串**（不再 JSON 解析）

### 3.6 tool response 消息映射

```
Chat:
{
  "role":"tool",
  "tool_call_id":"tc_1",
  "content":"72F"
}

→ Responses input[]:
{
  "type":"function_call_output",
  "call_id":"tc_1",
  "output":"72F"
}
```

**规则**：
- `tool` / `function` 两种 role 都映到 `function_call_output`
- `content` 字符串 → `output` 字符串
- `content` 多块 → `output` 是 JSON 序列化的 block 数组
- 缺 `tool_call_id` → 兜底当作 user 消息，内容带 `[tool_output_missing_call_id]` 前缀

### 3.7 顶层字段映射

| Chat `ChatRequest` 字段 | Responses 字段 | 规则 |
|---|---|---|
| `model` | `model` | 原样（必填） |
| `messages` | `input` | 按 3.3-3.6 |
| system/developer 消息 | `instructions` | 从 messages 中抽出，以 `\n\n` 连接 |
| `stream` | `stream` | 原样 |
| `temperature` | `temperature` | 原样 |
| `top_p` | `top_p` | 原样 |
| `max_tokens` / `max_completion_tokens` | `max_output_tokens` | 取**两者最大值**；两者皆空则 Responses 也不设 |
| `response_format` | `text` | 见 3.8 |
| `tools` | `tools` | 每个 `{type:"function",function:{...}}` → `{type:"function", name, description, parameters}`（**拍平**，去掉 `function` 嵌套） |
| `tool_choice` | `tool_choice` | `"auto"`/`"none"` 原样；`{type:"function",function:{name:"..."}}` → `{type:"function",name:"..."}`（拍平） |
| `parallel_tool_calls` | `parallel_tool_calls` | 原样 |
| `user` | `user` | 原样 |
| `store` | `store` | 原样 |
| `metadata` | `metadata` | 原样 |
| `reasoning_effort` | `reasoning: {effort, summary:"detailed"}` | 非空才设 |
| `n` | — | 若 n > 1 返错（不支持） |

### 3.8 response_format → text 字段

```
Chat:
{
  "response_format": {
    "type": "json_schema",
    "json_schema": {"name":"X","schema":{...},"strict":true}
  }
}

→ Responses:
{
  "text": {
    "format": {
      "type": "json_schema",
      "name": "X",
      "schema": {...},
      "strict": true
    }
  }
}
```

**规则**：`response_format.json_schema` 里的字段**提升**到 `text.format` 同级（扁平化）；嵌套的 `json_schema` key 删除。

### 3.9 Responses → Chat（镜像方向）

把上游 Responses API 返回的响应反向翻译成 canonical ChatResponse：

- `input` items 合并成 `messages`（system/developer items → system role message）
- `function_call` item + `function_call_output` item → assistant 的 `tool_calls` + 独立 tool message
- `text.format` → `response_format`（反向扁平化）
- `max_output_tokens` → `max_tokens`
- `reasoning.effort` → `reasoning_effort`

（具体翻译见实现时对 Responses wire 的逆向 walk through。）

---

## 4. 边界情况 & 踩坑清单

### 4.1 流式响应里提前 finish_reason + 后送 usage

**场景**：某些 OpenAI 兼容上游的流，**先**发 `finish_reason` chunk，**后**发只含 `usage` 的 chunk。

**处理**：
- 收到 `finish_reason != None` 但 `usage == None`：**不立即发 message_delta/stop**，返空，等后面 usage-only chunk 到了再关闭
- 收到 `len(choices) == 0 && usage != None`：用累积的 finish_reason 关闭

### 4.2 UTF-8 在 SSE chunk 边界断裂

`reqwest::bytes_stream` 按 TCP 包切，一个多字节字符可能拆到两 chunk。

**处理**：stream driver 维护一个 `chunk_buffer: Vec<u8>`，每次追加 bytes，按 `\n\n` 分割完整 SSE event，把**不完整的尾巴**留在 buffer 里（不 UTF-8 解码）。

### 4.3 工具调用的 arguments 流式拼接

OpenAI 流式 tool call 的 `function.arguments` 是**增量拼接**（一个 chunk 一小段字符）。Claude SSE 里每次发一个 `input_json_delta`，**不要累积**到 object（Claude 自家客户端负责拼）。

### 4.4 空 content + 空 tool_calls 的 chunk

不产出任何客户端事件（isEmpty 处理）。NewAPI `convert.go:571-573` 设 `isEmpty = true` 跳过。

### 4.5 Claude tool_result 的 name 反查

Claude `tool_result` 不带 tool name，canonical `role:"tool"` 消息需要 `name` 字段。**反查**：维护一个 `HashMap<String, String>` 从 `tool_use_id → name`，来源是历史消息里 assistant 发起的 `tool_calls`。

### 4.6 Gemini tool_call 的 id 一致性

Gemini 的 tool 调用没有 `id`，我们用 `call_${i+1}` 生成。但在**同一次会话**里，`functionCall` 和 `functionResponse` 要用**同一个 id**——转 canonical 时按**出现顺序**同步编号。

### 4.7 Responses 的 n > 1 不支持

直接在 `ChatCompletionsRequestToResponsesRequest` 返错：`"n>1 is not supported in responses compatibility mode"`。

### 4.8 max_output_tokens 最小值

Responses API 对 `max_output_tokens < 16` 会拒绝。NewAPI 注释掉了限制代码，**我们实施时保持同样放行**（上游报错更清晰）。

### 4.9 Stream 中途错误透传

流式响应中上游报错（HTTP 500 / 网络中断），要给客户端发一个 Claude 格式的错误 event 或 Gemini 的错误 JSON，不能让 SSE 连接静默断开。每种入口协议有自己的错误事件格式。

---

## 5. 错误传递约定

入口协议的**错误响应格式**也不同，EgressConverter 要负责：

### 5.1 OpenAI 入口

```json
{"error": {"message": "...", "type": "invalid_request_error", "code": "..."}}
```

### 5.2 Claude 入口

```json
{"type":"error","error":{"type":"invalid_request_error","message":"..."}}
```

流中错误：

```
event: error
data: {"type":"error","error":{...}}
```

### 5.3 Gemini 入口

```json
{"error":{"code":400,"message":"...","status":"INVALID_ARGUMENT"}}
```

### 5.4 Responses 入口

沿用 OpenAI 格式。

---

## 6. Rust 实现约定

### 6.1 Trait 签名

```rust
pub trait IngressConverter {
    type ClientRequest: DeserializeOwned;
    type ClientResponse: Serialize;
    type ClientStreamEvent: Serialize;

    const FORMAT: IngressFormat;

    fn to_canonical(req: Self::ClientRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest>;
    fn from_canonical(resp: ChatResponse, ctx: &IngressCtx) -> AdapterResult<Self::ClientResponse>;
    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>>;  // 注意返 Vec（一个 canonical event 可能→多个 Claude event）
}

pub struct IngressCtx {
    pub channel_kind: AdapterKind,       // 上游是谁（影响 cache_control 保留 / -thinking 后缀 / reasoning 方言）
    pub channel_vendor_code: String,
    pub logical_model: String,
    pub actual_model: String,
    pub support_stream_options: bool,
    pub estimated_prompt_tokens: u32,
}
```

### 6.2 StreamConvertState 每个入口协议自己定义

```rust
pub enum StreamConvertState {
    Openai(OpenAIStreamState),
    Claude(ClaudeStreamState),  // 见 §1.7.2
    Gemini(GeminiStreamState),
    Responses(ResponsesStreamState),
}
```

### 6.3 位置分工

- **入口 wire 类型定义**：`core/src/types/ingress_wire/{claude,gemini,openai_resp}.rs` — 纯 struct，无逻辑
- **转换纯函数 / trait impl**：`relay/src/convert/ingress/{openai,claude,gemini,openai_resp}.rs` + `egress/...` 同构
- **ReasonMap / NormalizeCacheCreationSplit 等小工具**：`relay/src/convert/common.rs`

### 6.4 测试约定

每个 converter 必须有 **round-trip 测试**：`canonical → client_wire → canonical` 语义等价（除了无损字段顺序）。至少这些 case：

- 纯文本 user 消息
- 多轮含 tool_use + tool_result
- 多模态（image + text）
- system message
- 流式：文本 / 工具 / 混合 / 错误中断

---

## 7. 参考文件

**NewAPI 源码**（我们的主要参考）：
- `service/convert.go`（1007 行）— Claude/Gemini ↔ OpenAI 全量
- `service/openaicompat/chat_to_responses.go`（402 行）— Chat → Responses
- `relay/channel/openai/adaptor.go` 里 `ConvertClaudeRequest` / `ConvertGeminiRequest`（调用 convert.go）
- `relay/reasonmap/`  — finish_reason 映射表

**官方 API 文档**：
- OpenAI Chat Completions: https://platform.openai.com/docs/api-reference/chat
- OpenAI Responses: https://platform.openai.com/docs/api-reference/responses
- Anthropic Messages: https://docs.anthropic.com/en/api/messages
- Anthropic Streaming: https://docs.anthropic.com/en/api/messages-streaming
- Gemini GenerateContent: https://ai.google.dev/api/generate-content

---

## 变更日志

| 日期 | 修改 | 原因 |
|---|---|---|
| 2026-04-19 | 初版（照搬 NewAPI `convert.go` + `chat_to_responses.go`） | 确立三家入口协议映射规格 |
