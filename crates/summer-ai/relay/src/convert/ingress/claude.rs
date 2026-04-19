//! Claude Messages ↔ canonical 转换（请求、响应、流）。
//!
//! 流事件重组：把 canonical 的 5 种语义事件
//! (Start / TextDelta / ReasoningDelta / ToolCallDelta / End)
//! 翻译成 Claude 客户端期望的 SSE 序列：
//!
//! ```text
//! message_start
//!   → (content_block_start → content_block_delta* → content_block_stop)+
//!   → message_delta
//!   → message_stop
//! ```
//!
//! # 已知限制
//!
//! 1. **`cache_control` 丢失**：canonical `ContentPart::Text` 暂无 cache_control 字段，
//!    Claude 入的 cache_control 提示会被丢弃。接入 Claude adapter 时再扩展 canonical
//!    并补透传逻辑。
//! 2. **`thinking` 仅 Claude 上游透传**：其他上游（OpenRouter / OpenAI）的 thinking
//!    方言转换后续再做（通过 `ctx.channel_kind` 分派）。
//! 3. **`Image` 只支持 base64 `data:` URI**：Claude URL 图像 source 映射时直接用 URL，
//!    canonical `ImageUrl.url` 接受任一。
//! 4. **`Document` / `RedactedThinking` / `Thinking` blocks** 在 `to_canonical` 时忽略
//!    （只在 Claude 原生上游有意义）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use summer_ai_core::types::ingress_wire::claude::{
    ClaudeContent, ClaudeContentBlock, ClaudeImageSource, ClaudeMessage, ClaudeMessagesRequest,
    ClaudeResponse, ClaudeStopReason, ClaudeStreamContentBlock, ClaudeStreamDelta,
    ClaudeStreamEvent, ClaudeStreamMessageDelta, ClaudeStreamMessageStart, ClaudeSystem,
    ClaudeTool, ClaudeToolChoice, ClaudeToolResultContent, ClaudeUsage,
};
use summer_ai_core::{
    AdapterError, AdapterKind, AdapterResult, ChatMessage, ChatRequest, ChatResponse,
    ChatStreamEvent, ContentPart, FinishReason, ImageUrl, MessageContent, Role, Tool, ToolCall,
    ToolCallFunction, ToolChoice, ToolFunction,
};

use super::{
    ClaudeLastMessageType, ClaudeStreamState, IngressConverter, IngressCtx, IngressFormat,
    StreamConvertState,
};

/// Claude Messages 入口协议转换器。
pub struct ClaudeIngress;

impl IngressConverter for ClaudeIngress {
    type ClientRequest = ClaudeMessagesRequest;
    type ClientResponse = ClaudeResponse;
    type ClientStreamEvent = ClaudeStreamEvent;

    const FORMAT: IngressFormat = IngressFormat::Claude;

    fn to_canonical(req: Self::ClientRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest> {
        to_canonical_impl(req, ctx)
    }

    fn from_canonical(resp: ChatResponse, ctx: &IngressCtx) -> AdapterResult<Self::ClientResponse> {
        from_canonical_impl(resp, ctx)
    }

    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>> {
        let StreamConvertState::Claude(claude_state) = state else {
            return Err(AdapterError::Unsupported {
                adapter: "claude_ingress",
                feature: "stream_convert_state_mismatch",
            });
        };
        Ok(from_canonical_stream_event_impl(event, claude_state, ctx))
    }
}

// ---------------------------------------------------------------------------
// to_canonical
// ---------------------------------------------------------------------------

fn to_canonical_impl(req: ClaudeMessagesRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest> {
    let ClaudeMessagesRequest {
        model,
        messages,
        max_tokens,
        system,
        temperature,
        top_p,
        top_k,
        stop_sequences,
        stream,
        tools,
        tool_choice,
        thinking,
        metadata,
        extra: _, // 未覆盖字段先丢弃
    } = req;

    // ---- messages ----

    // 第一遍：收集 tool_use_id → name 映射（供 tool_result 反查）
    let tool_use_name_map = build_tool_use_name_map(&messages);

    // 第二遍：逐条展开成 canonical messages
    let mut canonical_messages: Vec<ChatMessage> = Vec::with_capacity(messages.len() + 2);

    // system → messages[0]
    if let Some(sys) = system {
        canonical_messages.push(ChatMessage::system(flatten_system(sys)));
    }

    for msg in messages {
        append_claude_message(msg, &tool_use_name_map, &mut canonical_messages)?;
    }

    // ---- tools ----

    let canonical_tools = if tools.is_empty() {
        None
    } else {
        Some(
            tools
                .into_iter()
                .map(claude_tool_to_canonical)
                .collect::<Vec<_>>(),
        )
    };

    // ---- tool_choice ----

    let canonical_tool_choice = tool_choice.map(claude_tool_choice_to_canonical);

    // ---- stop_sequences → stop ----

    let stop = match stop_sequences.len() {
        0 => None,
        1 => Some(serde_json::Value::String(
            stop_sequences.into_iter().next().unwrap(),
        )),
        _ => Some(serde_json::Value::Array(
            stop_sequences
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        )),
    };

    // ---- user 字段 ----

    let user = metadata.and_then(|m| m.user_id);

    // ---- 透传 top_k / thinking 到 extra（canonical 没这两个字段）----

    let mut extra = serde_json::Map::new();
    if let Some(k) = top_k {
        extra.insert(
            "top_k".to_string(),
            serde_json::Value::Number(serde_json::Number::from(k)),
        );
    }
    if let Some(thinking) = thinking {
        // 仅 Claude 原生上游透传；其他上游后续再按方言转换
        if ctx.channel_kind == AdapterKind::Claude {
            extra.insert(
                "thinking".to_string(),
                serde_json::to_value(&thinking).map_err(AdapterError::SerializeRequest)?,
            );
        } else {
            tracing::debug!(
                channel = %ctx.channel_kind.as_lower_str(),
                "dropping `thinking` field: not Claude upstream"
            );
        }
    }

    Ok(ChatRequest {
        model,
        messages: canonical_messages,
        temperature,
        top_p,
        n: None,
        stop,
        max_tokens: Some(max_tokens as i64),
        max_completion_tokens: None,
        frequency_penalty: None,
        presence_penalty: None,
        logit_bias: None,
        logprobs: None,
        top_logprobs: None,
        response_format: None,
        modalities: None,
        audio: None,
        prediction: None,
        tools: canonical_tools,
        tool_choice: canonical_tool_choice,
        parallel_tool_calls: None,
        stream,
        stream_options: None,
        reasoning_effort: None,
        seed: None,
        service_tier: None,
        user,
        metadata: None,
        store: None,
        web_search_options: None,
        extra,
    })
}

/// 第一遍扫描：`tool_use_id → function_name` 映射。
///
/// Claude `tool_result` 不带 tool 名，canonical `role:"tool"` 需要 `name` 字段，
/// 反查历史消息里的 `tool_use` blocks 取 name。
fn build_tool_use_name_map(messages: &[ClaudeMessage]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for msg in messages {
        let ClaudeContent::Blocks(blocks) = &msg.content else {
            continue;
        };
        for block in blocks {
            if let ClaudeContentBlock::ToolUse { id, name, .. } = block {
                map.insert(id.clone(), name.clone());
            }
        }
    }
    map
}

/// 把 Claude `system` 字段扁平化成一个字符串（多 block 用 `\n` 连接）。
///
/// `cache_control` 提示当前丢失（canonical `ContentPart` 暂不带）。
fn flatten_system(sys: ClaudeSystem) -> String {
    match sys {
        ClaudeSystem::Text(s) => s,
        ClaudeSystem::Blocks(blocks) => blocks
            .into_iter()
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// 把一条 `ClaudeMessage` 追加到 canonical messages 序列。
///
/// - assistant 消息：text + tool_use → 一条 `assistant` 消息（带 tool_calls）
/// - user 消息：tool_result 拆成独立 `role:"tool"` 前置 + 剩余 content 归到 user 消息
fn append_claude_message(
    msg: ClaudeMessage,
    tool_use_name_map: &HashMap<String, String>,
    out: &mut Vec<ChatMessage>,
) -> AdapterResult<()> {
    let ClaudeMessage { role, content } = msg;
    let role_enum = match role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        other => {
            return Err(AdapterError::Unsupported {
                adapter: "claude_ingress",
                feature: role_feature_name(other),
            });
        }
    };

    // 字符串 content → 直接一条消息
    let blocks = match content {
        ClaudeContent::Text(text) => {
            out.push(ChatMessage {
                role: role_enum,
                content: Some(MessageContent::Text(text)),
                refusal: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
                audio: None,
            });
            return Ok(());
        }
        ClaudeContent::Blocks(blocks) => blocks,
    };

    // 分类收集
    let mut text_buf = String::new();
    let mut parts: Vec<ContentPart> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut tool_results: Vec<(String, String)> = Vec::new(); // (tool_use_id, text-or-json)

    for block in blocks {
        match block {
            ClaudeContentBlock::Text { text, .. } => {
                if parts.is_empty() {
                    if !text_buf.is_empty() {
                        text_buf.push('\n');
                    }
                    text_buf.push_str(&text);
                } else {
                    parts.push(ContentPart::Text { text });
                }
            }
            ClaudeContentBlock::Image { source, .. } => {
                // text_buf 里已有文本 → 提升成 Parts 混合
                if !text_buf.is_empty() {
                    parts.push(ContentPart::Text {
                        text: std::mem::take(&mut text_buf),
                    });
                }
                parts.push(ContentPart::ImageUrl {
                    image_url: claude_image_source_to_url(source),
                });
            }
            ClaudeContentBlock::ToolUse {
                id, name, input, ..
            } => {
                let arguments =
                    serde_json::to_string(&input).map_err(AdapterError::SerializeRequest)?;
                tool_calls.push(ToolCall {
                    id,
                    kind: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
            ClaudeContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error: _,
                ..
            } => {
                let body = claude_tool_result_to_string(content)?;
                tool_results.push((tool_use_id, body));
            }
            ClaudeContentBlock::Thinking { .. }
            | ClaudeContentBlock::RedactedThinking { .. }
            | ClaudeContentBlock::Document { .. } => {
                // 暂时忽略（需要 Claude 原生 adapter 配合才有意义）
            }
        }
    }

    // 先发 tool_results（拆成独立 role:"tool" 消息）——**必须在当前消息之前**
    for (tool_use_id, body) in tool_results {
        let name = tool_use_name_map.get(&tool_use_id).cloned();
        out.push(ChatMessage {
            role: Role::Tool,
            content: Some(MessageContent::Text(body)),
            refusal: None,
            name,
            tool_calls: None,
            tool_call_id: Some(tool_use_id),
            audio: None,
        });
    }

    // 决定当前消息的 content 形态
    let message_content: Option<MessageContent> = if !parts.is_empty() {
        if !text_buf.is_empty() {
            parts.insert(0, ContentPart::Text { text: text_buf });
        }
        Some(MessageContent::Parts(parts))
    } else if !text_buf.is_empty() {
        Some(MessageContent::Text(text_buf))
    } else if !tool_calls.is_empty() {
        // 有 tool_calls 但无文本 → content 留空（OpenAI 允许）
        None
    } else {
        // 全是 tool_result 没剩余 → 不再 push 一条空消息
        return Ok(());
    };

    out.push(ChatMessage {
        role: role_enum,
        content: message_content,
        refusal: None,
        name: None,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        tool_call_id: None,
        audio: None,
    });
    Ok(())
}

fn claude_image_source_to_url(source: ClaudeImageSource) -> ImageUrl {
    let url = match source {
        ClaudeImageSource::Base64 { media_type, data } => {
            format!("data:{media_type};base64,{data}")
        }
        ClaudeImageSource::Url { url } => url,
    };
    ImageUrl { url, detail: None }
}

/// `tool_result.content` 的两种形态 → canonical `content` 字符串。
fn claude_tool_result_to_string(content: Option<ClaudeToolResultContent>) -> AdapterResult<String> {
    match content {
        None => Ok(String::new()),
        Some(ClaudeToolResultContent::Text(s)) => Ok(s),
        Some(ClaudeToolResultContent::Blocks(blocks)) => {
            // 多块（text + image） → JSON-stringify 整个 block 数组
            serde_json::to_string(&blocks).map_err(AdapterError::SerializeRequest)
        }
    }
}

fn claude_tool_to_canonical(tool: ClaudeTool) -> Tool {
    Tool {
        kind: "function".to_string(),
        function: ToolFunction {
            name: tool.name,
            description: tool.description,
            parameters: Some(tool.input_schema),
        },
    }
}

fn claude_tool_choice_to_canonical(choice: ClaudeToolChoice) -> ToolChoice {
    match choice {
        ClaudeToolChoice::Auto { .. } => ToolChoice::Mode("auto".to_string()),
        ClaudeToolChoice::Any { .. } => ToolChoice::Mode("required".to_string()),
        ClaudeToolChoice::None => ToolChoice::Mode("none".to_string()),
        ClaudeToolChoice::Tool { name, .. } => ToolChoice::Named(serde_json::json!({
            "type": "function",
            "function": { "name": name }
        })),
    }
}

fn role_feature_name(role: &str) -> &'static str {
    match role {
        "system" => "role=system (use `system` field instead)",
        _ => "unknown_role",
    }
}

// ---------------------------------------------------------------------------
// from_canonical
// ---------------------------------------------------------------------------

fn from_canonical_impl(resp: ChatResponse, _ctx: &IngressCtx) -> AdapterResult<ClaudeResponse> {
    let ChatResponse {
        id,
        model,
        choices,
        usage,
        ..
    } = resp;

    // 取第一个 choice（Claude 响应只有一条 message）
    let (assistant, finish_reason) = match choices.into_iter().next() {
        Some(choice) => (choice.message, choice.finish_reason),
        None => {
            return Err(AdapterError::DeserializeResponse(
                serde_json::Error::custom("response has no choices"),
            ));
        }
    };

    // message.content + tool_calls → Vec<ClaudeContentBlock>
    let mut content_blocks: Vec<ClaudeContentBlock> = Vec::new();

    if let Some(text) = message_text(&assistant) {
        if !text.is_empty() {
            content_blocks.push(ClaudeContentBlock::Text {
                text,
                cache_control: None,
            });
        }
    }

    if let Some(tool_calls) = assistant.tool_calls {
        for tc in tool_calls {
            let input = match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                Ok(v) => v,
                Err(_) => serde_json::Value::String(tc.function.arguments.clone()),
            };
            content_blocks.push(ClaudeContentBlock::ToolUse {
                id: tc.id,
                name: tc.function.name,
                input,
                cache_control: None,
            });
        }
    }

    Ok(ClaudeResponse {
        id,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: content_blocks,
        model,
        stop_reason: Some(finish_reason_to_stop_reason(finish_reason)),
        stop_sequence: None,
        usage: usage_to_claude(usage),
    })
}

/// 取 `ChatMessage` 的第一段文本（兼容 Text/Parts）。
fn message_text(msg: &ChatMessage) -> Option<String> {
    let content = msg.content.as_ref()?;
    match content {
        MessageContent::Text(s) => Some(s.clone()),
        MessageContent::Parts(parts) => {
            let mut buf = String::new();
            for part in parts {
                if let ContentPart::Text { text } = part {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(text);
                }
            }
            if buf.is_empty() { None } else { Some(buf) }
        }
    }
}

/// finish_reason ↔ stop_reason 映射表。
fn finish_reason_to_stop_reason(reason: Option<FinishReason>) -> ClaudeStopReason {
    match reason {
        Some(FinishReason::Stop) => ClaudeStopReason::EndTurn,
        Some(FinishReason::Length) => ClaudeStopReason::MaxTokens,
        Some(FinishReason::ToolCalls) | Some(FinishReason::FunctionCall) => {
            ClaudeStopReason::ToolUse
        }
        Some(FinishReason::ContentFilter) => ClaudeStopReason::StopSequence,
        None => ClaudeStopReason::EndTurn, // Claude 要非空
    }
}

fn usage_to_claude(usage: summer_ai_core::Usage) -> ClaudeUsage {
    let cache_read = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens)
        .map(|v| v as u32);

    ClaudeUsage {
        input_tokens: usage.prompt_tokens.max(0) as u32,
        output_tokens: usage.completion_tokens.max(0) as u32,
        cache_creation_input_tokens: None, // canonical PromptTokensDetails 目前无此字段
        cache_read_input_tokens: cache_read,
        cache_creation: None,
        service_tier: None,
    }
}

// serde_json::Error::custom 是 trait method，需要 trait 在 scope
use serde::de::Error as _;

// ---------------------------------------------------------------------------
// from_canonical_stream_event —— 6-event 重组
// ---------------------------------------------------------------------------

/// 把 canonical 的语义事件翻译成 Claude SSE 序列。一次调用可能产出多个事件。
///
/// 规则：
/// - `Start` 首次到 → 发 `message_start`；后续 Start 忽略
/// - `TextDelta` → 若当前 block 不是 Text，先停旧的（content_block_stop + index++）
///   再发 `content_block_start{text}`，然后发 `content_block_delta{text_delta}`
/// - `ReasoningDelta` → 同理但 block 类型是 thinking
/// - `ToolCallDelta` → 若当前不是 Tools，停旧的 + 记 base_index；每个 index 首次见到
///   `name` 发 `content_block_start{tool_use}`，每次收到 `arguments_delta` 发
///   `content_block_delta{input_json_delta}`
/// - `End` → 停所有打开的 block + `message_delta` + `message_stop`
fn from_canonical_stream_event_impl(
    event: ChatStreamEvent,
    state: &mut ClaudeStreamState,
    ctx: &IngressCtx,
) -> Vec<ClaudeStreamEvent> {
    if state.done {
        return Vec::new();
    }

    let mut out = Vec::new();
    match event {
        ChatStreamEvent::Start { model, .. } => {
            ensure_message_start(&mut out, state, ctx, Some(model));
        }
        ChatStreamEvent::TextDelta { text } => {
            ensure_message_start(&mut out, state, ctx, None);
            if state.last_message_type != ClaudeLastMessageType::Text {
                stop_and_advance(&mut out, state);
                out.push(content_block_start_text(state.index as u32));
                state.last_message_type = ClaudeLastMessageType::Text;
            }
            out.push(content_block_delta(
                state.index as u32,
                ClaudeStreamDelta::TextDelta { text },
            ));
        }
        ChatStreamEvent::ReasoningDelta { text } => {
            ensure_message_start(&mut out, state, ctx, None);
            if state.last_message_type != ClaudeLastMessageType::Thinking {
                stop_and_advance(&mut out, state);
                out.push(content_block_start_thinking(state.index as u32));
                state.last_message_type = ClaudeLastMessageType::Thinking;
            }
            out.push(content_block_delta(
                state.index as u32,
                ClaudeStreamDelta::ThinkingDelta { thinking: text },
            ));
        }
        ChatStreamEvent::ToolCallDelta(delta) => {
            ensure_message_start(&mut out, state, ctx, None);
            if state.last_message_type != ClaudeLastMessageType::Tools {
                stop_and_advance(&mut out, state);
                state.tool_call_base_index = state.index;
                state.tool_call_max_index_offset = 0;
                state.last_message_type = ClaudeLastMessageType::Tools;
            }
            let offset = delta.index;
            if offset > state.tool_call_max_index_offset {
                state.tool_call_max_index_offset = offset;
            }
            let block_index = (state.tool_call_base_index + offset) as u32;

            // 首次见到 name → content_block_start
            if let Some(name) = delta.name {
                let id = delta.id.unwrap_or_else(|| format!("toolu_{}", block_index));
                out.push(content_block_start_tool_use(block_index, id, name));
            }
            // arguments 增量（可能与 name 同一次到达）
            if let Some(args) = delta.arguments_delta {
                if !args.is_empty() {
                    out.push(content_block_delta(
                        block_index,
                        ClaudeStreamDelta::InputJsonDelta { partial_json: args },
                    ));
                }
            }
        }
        ChatStreamEvent::End(end) => {
            ensure_message_start(&mut out, state, ctx, None);
            push_stop_open_blocks(&mut out, state);
            state.last_message_type = ClaudeLastMessageType::None;

            if let Some(new_usage) = end.usage {
                state.usage = Some(new_usage);
            }

            out.push(ClaudeStreamEvent::MessageDelta {
                delta: ClaudeStreamMessageDelta {
                    stop_reason: Some(finish_reason_to_stop_reason(end.finish_reason)),
                    stop_sequence: None,
                },
                usage: state.usage.as_ref().map(|u| ClaudeUsage {
                    input_tokens: u.prompt_tokens.max(0) as u32,
                    output_tokens: u.completion_tokens.max(0) as u32,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: u
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(|d| d.cached_tokens)
                        .map(|v| v as u32),
                    cache_creation: None,
                    service_tier: None,
                }),
            });
            out.push(ClaudeStreamEvent::MessageStop);
            state.done = true;
        }
    }

    state.send_response_count = state.send_response_count.saturating_add(out.len() as u32);
    out
}

/// 首次产出事件前必须先发 `message_start`。
fn ensure_message_start(
    out: &mut Vec<ClaudeStreamEvent>,
    state: &mut ClaudeStreamState,
    ctx: &IngressCtx,
    override_model: Option<String>,
) {
    if state.send_response_count > 0 {
        return;
    }
    let model = override_model.unwrap_or_else(|| ctx.actual_model.clone());
    out.push(ClaudeStreamEvent::MessageStart {
        message: ClaudeStreamMessageStart {
            id: generate_message_id(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: Vec::new(),
            model,
            stop_reason: None,
            stop_sequence: None,
            usage: ClaudeUsage {
                input_tokens: ctx.estimated_prompt_tokens,
                output_tokens: 0,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                cache_creation: None,
                service_tier: None,
            },
        },
    });
}

/// 关闭当前打开的 block，并为下一个 block 推进 index。
fn stop_and_advance(out: &mut Vec<ClaudeStreamEvent>, state: &mut ClaudeStreamState) {
    push_stop_open_blocks(out, state);
    match state.last_message_type {
        ClaudeLastMessageType::Tools => {
            state.index = state.tool_call_base_index + state.tool_call_max_index_offset + 1;
        }
        ClaudeLastMessageType::Text | ClaudeLastMessageType::Thinking => {
            state.index += 1;
        }
        ClaudeLastMessageType::None => {}
    }
    state.last_message_type = ClaudeLastMessageType::None;
}

/// 给当前打开的 block 发 content_block_stop（tool 并发时发多个）。
fn push_stop_open_blocks(out: &mut Vec<ClaudeStreamEvent>, state: &ClaudeStreamState) {
    match state.last_message_type {
        ClaudeLastMessageType::None => {}
        ClaudeLastMessageType::Text | ClaudeLastMessageType::Thinking => {
            out.push(ClaudeStreamEvent::ContentBlockStop {
                index: state.index as u32,
            });
        }
        ClaudeLastMessageType::Tools => {
            let base = state.tool_call_base_index;
            let max = state.tool_call_max_index_offset;
            for i in 0..=max {
                out.push(ClaudeStreamEvent::ContentBlockStop {
                    index: (base + i) as u32,
                });
            }
        }
    }
}

fn content_block_start_text(index: u32) -> ClaudeStreamEvent {
    ClaudeStreamEvent::ContentBlockStart {
        index,
        content_block: ClaudeStreamContentBlock::Text {
            text: String::new(),
        },
    }
}

fn content_block_start_thinking(index: u32) -> ClaudeStreamEvent {
    ClaudeStreamEvent::ContentBlockStart {
        index,
        content_block: ClaudeStreamContentBlock::Thinking {
            thinking: String::new(),
        },
    }
}

fn content_block_start_tool_use(index: u32, id: String, name: String) -> ClaudeStreamEvent {
    ClaudeStreamEvent::ContentBlockStart {
        index,
        content_block: ClaudeStreamContentBlock::ToolUse {
            id,
            name,
            input: serde_json::Value::Object(serde_json::Map::new()),
        },
    }
}

fn content_block_delta(index: u32, delta: ClaudeStreamDelta) -> ClaudeStreamEvent {
    ClaudeStreamEvent::ContentBlockDelta { index, delta }
}

/// 生成一个本地唯一的 `msg_` id（单调递增 + 时间戳前缀）。
fn generate_message_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("msg_{ts:x}{seq:x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::{ChatChoice, Usage};

    fn ctx() -> IngressCtx {
        IngressCtx::new(
            AdapterKind::Claude,
            "claude-sonnet-4-5",
            "claude-sonnet-4-5",
        )
    }

    // ───── to_canonical ─────

    #[test]
    fn minimal_request_to_canonical() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.model, "claude-sonnet-4-5");
        assert_eq!(canonical.max_tokens, Some(64));
        assert_eq!(canonical.messages.len(), 1);
        assert_eq!(canonical.messages[0].role, Role::User);
        assert!(matches!(
            canonical.messages[0].content.as_ref().unwrap(),
            MessageContent::Text(t) if t == "hi"
        ));
    }

    #[test]
    fn system_string_becomes_system_message() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "system": "you are helpful",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.messages.len(), 2);
        assert_eq!(canonical.messages[0].role, Role::System);
        assert!(matches!(
            canonical.messages[0].content.as_ref().unwrap(),
            MessageContent::Text(t) if t == "you are helpful"
        ));
    }

    #[test]
    fn system_blocks_joined_by_newline() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "system": [
                {"type": "text", "text": "line1"},
                {"type": "text", "text": "line2"}
            ],
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        let sys_content = canonical.messages[0].content.as_ref().unwrap();
        match sys_content {
            MessageContent::Text(t) => assert_eq!(t, "line1\nline2"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn stop_sequences_one_becomes_string_many_becomes_array() {
        let one: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "stop_sequences": ["END"],
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let c1 = ClaudeIngress::to_canonical(one, &ctx()).unwrap();
        assert_eq!(c1.stop, Some(serde_json::json!("END")));

        let many: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "stop_sequences": ["A", "B"],
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let c2 = ClaudeIngress::to_canonical(many, &ctx()).unwrap();
        assert_eq!(c2.stop, Some(serde_json::json!(["A", "B"])));
    }

    #[test]
    fn tool_use_promoted_to_tool_calls() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [
                {"role": "user", "content": "check weather"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "let me check"},
                    {"type": "tool_use", "id": "tu_1", "name": "weather", "input": {"city": "NYC"}}
                ]}
            ]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.messages.len(), 2);

        let assistant = &canonical.messages[1];
        assert_eq!(assistant.role, Role::Assistant);
        let tcs = assistant.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "tu_1");
        assert_eq!(tcs[0].function.name, "weather");
        assert_eq!(tcs[0].function.arguments, r#"{"city":"NYC"}"#);
        // text 仍保留
        match assistant.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "let me check"),
            _ => panic!("expected Text content"),
        }
    }

    #[test]
    fn tool_result_splits_into_tool_message_and_user_remainder() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [
                {"role": "user", "content": "check"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tu_1", "name": "weather", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1", "content": "72F"},
                    {"type": "text", "text": "what's next"}
                ]}
            ]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        // user / assistant / tool / user
        assert_eq!(canonical.messages.len(), 4);

        let tool_msg = &canonical.messages[2];
        assert_eq!(tool_msg.role, Role::Tool);
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("tu_1"));
        assert_eq!(tool_msg.name.as_deref(), Some("weather"));
        match tool_msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "72F"),
            _ => panic!("expected Text"),
        }

        let user_after = &canonical.messages[3];
        assert_eq!(user_after.role, Role::User);
        match user_after.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "what's next"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn image_content_becomes_data_uri() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "describe"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "XYZ"}}
            ]}]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        match canonical.messages[0].content.as_ref().unwrap() {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                if let ContentPart::ImageUrl { image_url } = &parts[1] {
                    assert!(image_url.url.starts_with("data:image/png;base64,"));
                    assert!(image_url.url.ends_with("XYZ"));
                } else {
                    panic!("expected ImageUrl part");
                }
            }
            _ => panic!("expected Parts"),
        }
    }

    #[test]
    fn tool_choice_maps_all_variants() {
        let make = |tc: serde_json::Value| -> ChatRequest {
            let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
                "model": "claude-sonnet-4-5",
                "max_tokens": 64,
                "tool_choice": tc,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .unwrap();
            ClaudeIngress::to_canonical(req, &ctx()).unwrap()
        };

        match make(serde_json::json!({"type": "auto"}))
            .tool_choice
            .unwrap()
        {
            ToolChoice::Mode(s) => assert_eq!(s, "auto"),
            _ => panic!(),
        }
        match make(serde_json::json!({"type": "any"}))
            .tool_choice
            .unwrap()
        {
            ToolChoice::Mode(s) => assert_eq!(s, "required"),
            _ => panic!(),
        }
        match make(serde_json::json!({"type": "none"}))
            .tool_choice
            .unwrap()
        {
            ToolChoice::Mode(s) => assert_eq!(s, "none"),
            _ => panic!(),
        }
        match make(serde_json::json!({"type": "tool", "name": "weather"}))
            .tool_choice
            .unwrap()
        {
            ToolChoice::Named(v) => {
                assert_eq!(v["type"], "function");
                assert_eq!(v["function"]["name"], "weather");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn thinking_transparent_only_for_anthropic_upstream() {
        let req_json = serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [{"role": "user", "content": "hi"}]
        });

        // Claude upstream → 透传到 extra
        let req: ClaudeMessagesRequest = serde_json::from_value(req_json.clone()).unwrap();
        let c1 = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        assert!(c1.extra.contains_key("thinking"));

        // 非 Claude upstream → 丢弃
        let req: ClaudeMessagesRequest = serde_json::from_value(req_json).unwrap();
        let mut non_anthropic_ctx = ctx();
        non_anthropic_ctx.channel_kind = AdapterKind::OpenAI;
        let c2 = ClaudeIngress::to_canonical(req, &non_anthropic_ctx).unwrap();
        assert!(!c2.extra.contains_key("thinking"));
    }

    #[test]
    fn metadata_user_id_maps_to_user_field() {
        let req: ClaudeMessagesRequest = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 64,
            "metadata": {"user_id": "u-123"},
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();
        let canonical = ClaudeIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.user.as_deref(), Some("u-123"));
    }

    // ───── from_canonical ─────

    #[test]
    fn basic_response_from_canonical() {
        let resp = ChatResponse {
            id: "chatcmpl-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "claude-sonnet-4-5".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage::assistant("hello"),
                logprobs: None,
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: Usage {
                prompt_tokens: 5,
                completion_tokens: 2,
                total_tokens: 7,
                ..Default::default()
            },
            system_fingerprint: None,
            service_tier: None,
        };
        let claude = ClaudeIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(claude.id, "chatcmpl-1");
        assert_eq!(claude.role, "assistant");
        assert_eq!(claude.content.len(), 1);
        match &claude.content[0] {
            ClaudeContentBlock::Text { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("expected Text"),
        }
        assert_eq!(claude.stop_reason, Some(ClaudeStopReason::EndTurn));
        assert_eq!(claude.usage.input_tokens, 5);
        assert_eq!(claude.usage.output_tokens, 2);
    }

    #[test]
    fn tool_calls_response_emits_tool_use_blocks() {
        let resp = ChatResponse {
            id: "chatcmpl-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "claude-sonnet-4-5".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("let me check".to_string())),
                    refusal: None,
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "tc_1".to_string(),
                        kind: "function".to_string(),
                        function: ToolCallFunction {
                            name: "weather".to_string(),
                            arguments: r#"{"city":"NYC"}"#.to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    audio: None,
                },
                logprobs: None,
                finish_reason: Some(FinishReason::ToolCalls),
            }],
            usage: Usage::default(),
            system_fingerprint: None,
            service_tier: None,
        };
        let claude = ClaudeIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(claude.content.len(), 2);
        match &claude.content[0] {
            ClaudeContentBlock::Text { text, .. } => assert_eq!(text, "let me check"),
            _ => panic!("expected Text"),
        }
        match &claude.content[1] {
            ClaudeContentBlock::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "tc_1");
                assert_eq!(name, "weather");
                assert_eq!(input["city"], "NYC");
            }
            _ => panic!("expected ToolUse"),
        }
        assert_eq!(claude.stop_reason, Some(ClaudeStopReason::ToolUse));
    }

    #[test]
    fn finish_reason_mapping_table() {
        use FinishReason::*;
        assert_eq!(
            finish_reason_to_stop_reason(Some(Stop)),
            ClaudeStopReason::EndTurn
        );
        assert_eq!(
            finish_reason_to_stop_reason(Some(Length)),
            ClaudeStopReason::MaxTokens
        );
        assert_eq!(
            finish_reason_to_stop_reason(Some(ToolCalls)),
            ClaudeStopReason::ToolUse
        );
        assert_eq!(
            finish_reason_to_stop_reason(Some(FunctionCall)),
            ClaudeStopReason::ToolUse
        );
        assert_eq!(
            finish_reason_to_stop_reason(Some(ContentFilter)),
            ClaudeStopReason::StopSequence
        );
        assert_eq!(
            finish_reason_to_stop_reason(None),
            ClaudeStopReason::EndTurn
        );
    }

    #[test]
    fn usage_cached_tokens_maps_to_cache_read() {
        use summer_ai_core::PromptTokensDetails;
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: Some(80),
                audio_tokens: None,
            }),
            ..Default::default()
        };
        let c = usage_to_claude(usage);
        assert_eq!(c.input_tokens, 100);
        assert_eq!(c.output_tokens, 50);
        assert_eq!(c.cache_read_input_tokens, Some(80));
    }

    // ───── from_canonical_stream_event ─────

    fn stream_ctx() -> IngressCtx {
        let mut c = ctx();
        c.estimated_prompt_tokens = 42;
        c
    }

    fn init_state() -> StreamConvertState {
        StreamConvertState::for_format(IngressFormat::Claude)
    }

    fn run(
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
        event: ChatStreamEvent,
    ) -> Vec<ClaudeStreamEvent> {
        ClaudeIngress::from_canonical_stream_event(event, state, ctx).unwrap()
    }

    #[test]
    fn stream_first_text_emits_message_start_block_start_and_delta() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "hi".to_string(),
            },
        );
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], ClaudeStreamEvent::MessageStart { .. }));
        match &out[0] {
            ClaudeStreamEvent::MessageStart { message } => {
                assert_eq!(message.usage.input_tokens, 42);
                assert_eq!(message.role, "assistant");
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            out[1],
            ClaudeStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ClaudeStreamContentBlock::Text { .. }
            }
        ));
        match &out[2] {
            ClaudeStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(*index, 0);
                match delta {
                    ClaudeStreamDelta::TextDelta { text } => assert_eq!(text, "hi"),
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_start_event_picks_up_model() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::Start {
                adapter: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
            },
        );
        assert_eq!(out.len(), 1);
        match &out[0] {
            ClaudeStreamEvent::MessageStart { message } => {
                assert_eq!(message.model, "gpt-4o-mini");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_subsequent_text_deltas_reuse_block() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let _ = run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "a".to_string(),
            },
        );
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "b".to_string(),
            },
        );
        // 后续 delta 不应再发 message_start / content_block_start
        assert_eq!(out.len(), 1);
        match &out[0] {
            ClaudeStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(*index, 0);
                match delta {
                    ClaudeStreamDelta::TextDelta { text } => assert_eq!(text, "b"),
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_reasoning_then_text_closes_thinking_and_opens_text() {
        let ctx = stream_ctx();
        let mut state = init_state();
        run(
            &mut state,
            &ctx,
            ChatStreamEvent::ReasoningDelta {
                text: "hm".to_string(),
            },
        );
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "hi".to_string(),
            },
        );
        // 关闭 thinking(0) + 开 text(1) + delta
        assert_eq!(out.len(), 3);
        match &out[0] {
            ClaudeStreamEvent::ContentBlockStop { index } => assert_eq!(*index, 0),
            _ => panic!(),
        }
        match &out[1] {
            ClaudeStreamEvent::ContentBlockStart {
                index,
                content_block: ClaudeStreamContentBlock::Text { .. },
            } => assert_eq!(*index, 1),
            _ => panic!(),
        }
        assert!(matches!(
            out[2],
            ClaudeStreamEvent::ContentBlockDelta { .. }
        ));
    }

    #[test]
    fn stream_tool_call_delta_emits_tool_use_and_input_json() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::ToolCallDelta(summer_ai_core::ToolCallDelta {
                index: 0,
                id: Some("tc_1".to_string()),
                name: Some("weather".to_string()),
                arguments_delta: Some(r#"{"city""#.to_string()),
            }),
        );
        // message_start + content_block_start{tool_use} + content_block_delta{input_json_delta}
        assert_eq!(out.len(), 3);
        match &out[1] {
            ClaudeStreamEvent::ContentBlockStart {
                index,
                content_block: ClaudeStreamContentBlock::ToolUse { id, name, .. },
            } => {
                assert_eq!(*index, 0);
                assert_eq!(id, "tc_1");
                assert_eq!(name, "weather");
            }
            _ => panic!(),
        }
        match &out[2] {
            ClaudeStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(*index, 0);
                match delta {
                    ClaudeStreamDelta::InputJsonDelta { partial_json } => {
                        assert!(partial_json.starts_with("{\"city"));
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }

        // 后续 tool_call delta 只带 arguments
        let out2 = run(
            &mut state,
            &ctx,
            ChatStreamEvent::ToolCallDelta(summer_ai_core::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments_delta: Some(":\"NYC\"}".to_string()),
            }),
        );
        assert_eq!(out2.len(), 1);
        assert!(matches!(
            out2[0],
            ClaudeStreamEvent::ContentBlockDelta {
                delta: ClaudeStreamDelta::InputJsonDelta { .. },
                ..
            }
        ));
    }

    #[test]
    fn stream_tool_call_missing_id_autogens() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::ToolCallDelta(summer_ai_core::ToolCallDelta {
                index: 0,
                id: None,
                name: Some("weather".to_string()),
                arguments_delta: None,
            }),
        );
        match &out[1] {
            ClaudeStreamEvent::ContentBlockStart {
                content_block: ClaudeStreamContentBlock::ToolUse { id, .. },
                ..
            } => assert!(id.starts_with("toolu_")),
            _ => panic!(),
        }
    }

    #[test]
    fn stream_parallel_tool_calls_track_max_offset() {
        let ctx = stream_ctx();
        let mut state = init_state();
        // 打开 tool 0
        run(
            &mut state,
            &ctx,
            ChatStreamEvent::ToolCallDelta(summer_ai_core::ToolCallDelta {
                index: 0,
                id: Some("tc_0".to_string()),
                name: Some("a".to_string()),
                arguments_delta: None,
            }),
        );
        // 打开 tool 1（同一批次并发）
        run(
            &mut state,
            &ctx,
            ChatStreamEvent::ToolCallDelta(summer_ai_core::ToolCallDelta {
                index: 1,
                id: Some("tc_1".to_string()),
                name: Some("b".to_string()),
                arguments_delta: None,
            }),
        );
        // End → 应该给 tool 0 和 tool 1 都发 content_block_stop
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::End(summer_ai_core::StreamEnd {
                finish_reason: Some(FinishReason::ToolCalls),
                usage: None,
            }),
        );
        let stops: Vec<_> = out
            .iter()
            .filter_map(|e| match e {
                ClaudeStreamEvent::ContentBlockStop { index } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(stops, vec![0, 1]);
    }

    #[test]
    fn stream_end_emits_message_delta_and_stop() {
        let ctx = stream_ctx();
        let mut state = init_state();
        run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "hi".to_string(),
            },
        );
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::End(summer_ai_core::StreamEnd {
                finish_reason: Some(FinishReason::Stop),
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                    ..Default::default()
                }),
            }),
        );
        // content_block_stop(0) + message_delta + message_stop
        assert_eq!(out.len(), 3);
        assert!(matches!(
            out[0],
            ClaudeStreamEvent::ContentBlockStop { index: 0 }
        ));
        match &out[1] {
            ClaudeStreamEvent::MessageDelta { delta, usage } => {
                assert_eq!(delta.stop_reason, Some(ClaudeStopReason::EndTurn));
                let usage = usage.as_ref().unwrap();
                assert_eq!(usage.output_tokens, 20);
                assert_eq!(usage.input_tokens, 10);
            }
            _ => panic!(),
        }
        assert!(matches!(out[2], ClaudeStreamEvent::MessageStop));

        // 进入 done 状态后继续喂事件返空
        let extra = run(
            &mut state,
            &ctx,
            ChatStreamEvent::TextDelta {
                text: "x".to_string(),
            },
        );
        assert!(extra.is_empty());
    }

    #[test]
    fn stream_end_without_prior_content_still_sends_message_start() {
        let ctx = stream_ctx();
        let mut state = init_state();
        let out = run(
            &mut state,
            &ctx,
            ChatStreamEvent::End(summer_ai_core::StreamEnd {
                finish_reason: Some(FinishReason::Stop),
                usage: None,
            }),
        );
        // message_start + message_delta + message_stop
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], ClaudeStreamEvent::MessageStart { .. }));
        assert!(matches!(out[1], ClaudeStreamEvent::MessageDelta { .. }));
        assert!(matches!(out[2], ClaudeStreamEvent::MessageStop));
    }

    #[test]
    fn stream_wrong_state_variant_errors() {
        let ctx = stream_ctx();
        let mut state = StreamConvertState::Openai;
        let err = ClaudeIngress::from_canonical_stream_event(
            ChatStreamEvent::TextDelta {
                text: "hi".to_string(),
            },
            &mut state,
            &ctx,
        );
        assert!(err.is_err());
    }
}
