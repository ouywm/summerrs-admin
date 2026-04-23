//! OpenAI Responses API (`/v1/responses`) ↔ canonical `ChatRequest` 转换。
//!
//! # 映射要点
//!
//! ## Request (to_canonical)
//!
//! - `input: string` → `[ChatMessage::user(string)]`
//! - `input: Items` → 逐项映射：
//!     - `Message` → 按 role 产出 `ChatMessage`（文本/多模态）
//!     - `FunctionCall` → assistant `ChatMessage` with `tool_calls`
//!     - `FunctionCallOutput` → `ChatMessage::tool_response(call_id, output)`
//!     - `Unknown` → 丢弃并 warn
//! - `instructions` → 若非空，在 messages 前插入一条 system 消息
//! - `tools`：function → canonical `Tool::function(...)`；built-in → `Tool::builtin(...)`
//! - `max_output_tokens` → canonical `max_completion_tokens`
//! - `stream` 位由外层 handler 覆盖，不在 converter 里动
//! - `reasoning.summary` / `previous_response_id` / `instructions` → `responses_extras`
//! - `store` / `metadata` 等已有 canonical 字段直接写入
//!
//! ## Response (from_canonical)
//!
//! - `ChatResponse.choices[0].message` 展开成 `output` 列表：
//!     - `tool_calls` 非空 → 每个调用一个 `OutputItem::FunctionCall`
//!     - `content` / `refusal` → 一个 `OutputItem::Message`
//! - `finish_reason` 映射 `status`：`Stop/ToolCalls/FunctionCall` → `completed`；
//!   `Length` → `incomplete`；其他 → `incomplete`
//! - `usage`：`prompt_tokens` → `input_tokens`；`completion_tokens` → `output_tokens`
//! - `output_text` 便利字段填所有 text part 拼接
//!
//! ## Stream (from_canonical_stream_event)
//!
//! 把 canonical `ChatStreamEvent` 翻译成 Responses API 的多层 SSE：
//! `response.created` → `response.in_progress` → (每个 output item 的
//! `output_item.added` → content/function_call delta → `output_item.done`)
//! → `response.completed`。文本与 tool_call 不并存，切换时自动关闭前一个
//! item。`sequence_number` 从 0 单调递增，`reasoning` delta 本轮忽略。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use summer_ai_core::types::ingress_wire::openai_responses::{
    OpenAIResponsesFunctionCallItem, OpenAIResponsesFunctionCallOutputItem,
    OpenAIResponsesFunctionTool, OpenAIResponsesInput, OpenAIResponsesInputContentPart,
    OpenAIResponsesInputItem, OpenAIResponsesMessageContent, OpenAIResponsesMessageItem,
    OpenAIResponsesOutputContentPart, OpenAIResponsesOutputFunctionCall, OpenAIResponsesOutputItem,
    OpenAIResponsesOutputMessage, OpenAIResponsesRequest, OpenAIResponsesResponse,
    OpenAIResponsesStreamEvent, OpenAIResponsesTool, OpenAIResponsesUsage,
};
use summer_ai_core::{
    AdapterError, AdapterResult, ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent,
    ContentPart, FinishReason, ImageUrl, MessageContent, ReasoningEffort, ResponsesExtras, Role,
    Tool, ToolCall, ToolCallDelta, ToolCallFunction, ToolChoice, ToolFunction, Usage,
};

use super::{
    IngressConverter, IngressCtx, IngressFormat, OpenMessageState, OpenToolCallState,
    ResponsesStreamState, StreamConvertState,
};

/// OpenAI Responses 入口（`POST /v1/responses`）的 ingress / egress 转换器。
pub struct OpenAIResponsesIngress;

impl IngressConverter for OpenAIResponsesIngress {
    type ClientRequest = OpenAIResponsesRequest;
    type ClientResponse = OpenAIResponsesResponse;
    type ClientStreamEvent = OpenAIResponsesStreamEvent;

    const FORMAT: IngressFormat = IngressFormat::OpenAIResponses;

    fn to_canonical(req: Self::ClientRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest> {
        to_canonical_impl(req, ctx)
    }

    fn from_canonical(resp: ChatResponse, ctx: &IngressCtx) -> AdapterResult<Self::ClientResponse> {
        Ok(from_canonical_impl(resp, ctx))
    }

    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>> {
        let StreamConvertState::Responses(resp_state) = state else {
            return Err(AdapterError::Unsupported {
                adapter: "openai_responses_ingress",
                feature: "stream_convert_state_mismatch",
            });
        };
        Ok(from_canonical_stream_event_impl(event, resp_state, ctx))
    }
}

// =========================================================================
// to_canonical
// =========================================================================

fn to_canonical_impl(req: OpenAIResponsesRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest> {
    let OpenAIResponsesRequest {
        model: _,
        input,
        instructions,
        tools,
        tool_choice,
        temperature,
        top_p,
        max_output_tokens,
        stream: _,
        parallel_tool_calls,
        previous_response_id,
        reasoning,
        store,
        user,
        metadata,
        extra,
    } = req;
    let extra: serde_json::Map<String, serde_json::Value> = extra.into_iter().collect();

    let mut messages: Vec<ChatMessage> = Vec::new();
    let instructions = instructions.filter(|value| !value.is_empty());

    // 1) instructions → 前置 system 消息
    if let Some(text) = instructions.clone() {
        messages.push(ChatMessage::system(text));
    }

    // 2) input → messages
    match input {
        OpenAIResponsesInput::Text(text) => {
            messages.push(ChatMessage::user(text));
        }
        OpenAIResponsesInput::Items(items) => {
            for item in items {
                if let Some(msg) = input_item_to_chat_message(item)? {
                    messages.push(msg);
                }
            }
        }
    }

    // 3) tools
    let tools: Vec<Tool> = tools
        .into_iter()
        .map(|t| match t {
            OpenAIResponsesTool::Function(f) => function_tool_to_canonical(f),
            OpenAIResponsesTool::Builtin { kind, extra } => Tool::builtin(kind, extra),
        })
        .collect();

    let tool_choice: Option<ToolChoice> =
        tool_choice.and_then(|v| serde_json::from_value::<ToolChoice>(v).ok());

    let (reasoning_effort, reasoning_summary) = match reasoning {
        Some(reasoning) => (
            reasoning
                .effort
                .as_deref()
                .and_then(ReasoningEffort::from_keyword),
            reasoning.summary,
        ),
        None => (None, None),
    };

    let responses_extras = if previous_response_id.is_some()
        || reasoning_summary.is_some()
        || instructions.is_some()
    {
        Some(ResponsesExtras {
            previous_response_id,
            reasoning_summary,
            instructions,
        })
    } else {
        None
    };

    Ok(ChatRequest {
        model: ctx.actual_model.clone(),
        messages,
        temperature,
        top_p,
        max_completion_tokens: max_output_tokens,
        parallel_tool_calls,
        reasoning_effort,
        user,
        metadata,
        store,
        tools: if tools.is_empty() { None } else { Some(tools) },
        tool_choice,
        responses_extras,
        extra,
        // stream 位由 handler 根据路径（`POST /v1/responses` 非流 vs 设 stream=true）覆盖
        stream: false,
        ..Default::default()
    })
}

fn input_item_to_chat_message(
    item: OpenAIResponsesInputItem,
) -> AdapterResult<Option<ChatMessage>> {
    Ok(match item {
        OpenAIResponsesInputItem::Message(m) => Some(message_item_to_chat_message(m)?),
        OpenAIResponsesInputItem::FunctionCall(fc) => Some(function_call_item_to_chat_message(fc)),
        OpenAIResponsesInputItem::FunctionCallOutput(fco) => {
            Some(function_call_output_to_chat_message(fco))
        }
        OpenAIResponsesInputItem::Unknown => {
            tracing::warn!("responses ingress: unsupported input item type, dropped");
            None
        }
    })
}

fn message_item_to_chat_message(m: OpenAIResponsesMessageItem) -> AdapterResult<ChatMessage> {
    let role = parse_role(&m.role)?;
    let content = match m.content {
        OpenAIResponsesMessageContent::Text(text) => MessageContent::text(text),
        OpenAIResponsesMessageContent::Parts(parts) => {
            let canonical_parts: Vec<ContentPart> = parts
                .into_iter()
                .filter_map(content_part_to_canonical)
                .collect();
            MessageContent::parts(canonical_parts)
        }
    };

    Ok(ChatMessage {
        role,
        content: Some(content),
        refusal: None,
        name: None,
        tool_calls: None,
        tool_call_id: None,
        reasoning_content: None,
        audio: None,
        options: None,
    })
}

fn parse_role(role: &str) -> AdapterResult<Role> {
    Ok(match role {
        "user" => Role::User,
        "system" => Role::System,
        "developer" => Role::Developer,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => {
            return Err(AdapterError::Unsupported {
                adapter: "OpenAIResp",
                feature: Box::leak(format!("role:{other}").into_boxed_str()),
            });
        }
    })
}

fn content_part_to_canonical(part: OpenAIResponsesInputContentPart) -> Option<ContentPart> {
    match part {
        OpenAIResponsesInputContentPart::InputText { text } => Some(ContentPart::Text { text }),
        OpenAIResponsesInputContentPart::OutputText { text, .. } => {
            Some(ContentPart::Text { text })
        }
        OpenAIResponsesInputContentPart::InputImage {
            image_url,
            file_id,
            detail,
        } => {
            // canonical 只有 url 形态；file_id 场景暂不支持
            let url = image_url.or_else(|| file_id.map(|id| format!("file://{id}")))?;
            Some(ContentPart::ImageUrl {
                image_url: ImageUrl { url, detail },
            })
        }
        OpenAIResponsesInputContentPart::InputFile {
            file_id,
            filename,
            file_data: _,
        } => {
            // canonical 暂无 file part；降级成文本占位
            let placeholder = match (file_id.as_deref(), filename.as_deref()) {
                (Some(id), Some(name)) => format!("[file {name} id={id}]"),
                (Some(id), None) => format!("[file id={id}]"),
                (None, Some(name)) => format!("[file {name}]"),
                (None, None) => "[file]".to_string(),
            };
            Some(ContentPart::Text { text: placeholder })
        }
        OpenAIResponsesInputContentPart::Unknown => {
            tracing::warn!("responses ingress: unsupported content part, dropped");
            None
        }
    }
}

fn function_tool_to_canonical(f: OpenAIResponsesFunctionTool) -> Tool {
    Tool::function(ToolFunction {
        name: f.name,
        description: f.description,
        parameters: f.parameters,
    })
}

fn function_call_item_to_chat_message(fc: OpenAIResponsesFunctionCallItem) -> ChatMessage {
    ChatMessage {
        role: Role::Assistant,
        content: None,
        reasoning_content: None,
        refusal: None,
        name: None,
        tool_calls: Some(vec![ToolCall {
            id: fc.call_id,
            kind: "function".to_string(),
            function: ToolCallFunction {
                name: fc.name,
                arguments: fc.arguments,
            },
            thought_signatures: None,
        }]),
        tool_call_id: None,
        audio: None,
        options: None,
    }
}

fn function_call_output_to_chat_message(fco: OpenAIResponsesFunctionCallOutputItem) -> ChatMessage {
    ChatMessage::tool_response(fco.call_id, fco.output)
}

// =========================================================================
// from_canonical
// =========================================================================

fn from_canonical_impl(resp: ChatResponse, ctx: &IngressCtx) -> OpenAIResponsesResponse {
    let mut output: Vec<OpenAIResponsesOutputItem> = Vec::new();
    let mut output_text_buf: String = String::new();
    let mut status = "completed".to_string();

    if let Some(choice) = resp.choices.into_iter().next() {
        // finish_reason → status
        status = match choice.finish_reason {
            Some(FinishReason::Stop)
            | Some(FinishReason::ToolCalls)
            | Some(FinishReason::FunctionCall) => "completed".into(),
            Some(FinishReason::Length) => "incomplete".into(),
            Some(FinishReason::ContentFilter) => "incomplete".into(),
            None => "completed".into(),
        };

        let msg = choice.message;

        // tool_calls → 每个一个 FunctionCall output item
        if let Some(tool_calls) = msg.tool_calls {
            for tc in tool_calls {
                output.push(OpenAIResponsesOutputItem::FunctionCall(
                    OpenAIResponsesOutputFunctionCall {
                        id: format!("fc_{}", tc.id),
                        call_id: tc.id,
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                        status: Some("completed".into()),
                    },
                ));
            }
        }

        // content / refusal → 单个 message output item
        let mut content_parts: Vec<OpenAIResponsesOutputContentPart> = Vec::new();
        if let Some(refusal) = msg.refusal {
            content_parts.push(OpenAIResponsesOutputContentPart::Refusal { refusal });
        } else if let Some(content) = msg.content {
            match content {
                MessageContent::Text(t) => {
                    output_text_buf.push_str(&t);
                    content_parts.push(OpenAIResponsesOutputContentPart::OutputText {
                        text: t,
                        annotations: Vec::new(),
                    });
                }
                MessageContent::Parts(parts) => {
                    for p in parts {
                        if let ContentPart::Text { text } = p {
                            output_text_buf.push_str(&text);
                            content_parts.push(OpenAIResponsesOutputContentPart::OutputText {
                                text,
                                annotations: Vec::new(),
                            });
                        }
                    }
                }
            }
        }

        if !content_parts.is_empty() {
            output.push(OpenAIResponsesOutputItem::Message(
                OpenAIResponsesOutputMessage {
                    id: format!("msg_{}", resp.id),
                    status: "completed".into(),
                    role: "assistant".into(),
                    content: content_parts,
                },
            ));
        }
    }

    OpenAIResponsesResponse {
        id: resp.id,
        object: "response".into(),
        created_at: resp.created,
        model: ctx.actual_model.clone(),
        status,
        output,
        usage: Some(OpenAIResponsesUsage {
            input_tokens: resp.usage.prompt_tokens,
            output_tokens: resp.usage.completion_tokens,
            total_tokens: resp.usage.total_tokens,
            input_tokens_details: None,
            output_tokens_details: None,
        }),
        output_text: if output_text_buf.is_empty() {
            None
        } else {
            Some(output_text_buf)
        },
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        temperature: None,
        top_p: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        tool_choice: None,
        tools: Vec::new(),
        reasoning: None,
        user: None,
        metadata: None,
        error: None,
    }
}

// =========================================================================
// from_canonical_stream_event —— Responses API SSE 重组
// =========================================================================

/// 把 canonical `ChatStreamEvent` 翻译成 Responses API SSE 事件。一次调用可能产出多个事件。
///
/// # 事件编排
///
/// ```text
/// (首事件) response.created + response.in_progress
/// │
/// ├── [文本流] output_item.added(message,in_progress)
/// │            + content_part.added(output_text,"")
/// │            + output_text.delta*
/// │            + output_text.done
/// │            + content_part.done
/// │            + output_item.done(message,completed)
/// │
/// ├── [工具调用] (对每个 index 并发)
/// │              output_item.added(function_call,in_progress)
/// │              + function_call_arguments.delta*
/// │              (End 时) function_call_arguments.done + output_item.done
/// │
/// └── (End 时) response.completed
/// ```
///
/// Reasoning 事件当前被忽略（Responses 的 `reasoning` output item 规格不稳定）。
fn from_canonical_stream_event_impl(
    event: ChatStreamEvent,
    state: &mut ResponsesStreamState,
    ctx: &IngressCtx,
) -> Vec<OpenAIResponsesStreamEvent> {
    if state.done {
        return Vec::new();
    }
    let mut out = Vec::new();
    match event {
        ChatStreamEvent::Start { model, .. } => {
            ensure_initialized(&mut out, state, ctx, Some(model));
        }
        ChatStreamEvent::TextDelta { text } => {
            ensure_initialized(&mut out, state, ctx, None);

            // 文本不能和 tool_call 并存：若当前开着 tool_call，全部关掉
            if !state.open_tool_calls.is_empty() {
                close_all_tool_calls(&mut out, state);
            }
            if state.open_message.is_none() {
                open_new_message(&mut out, state);
            }

            let (item_id, output_index, content_index) = {
                let msg = state.open_message.as_mut().expect("just opened");
                msg.text_buf.push_str(&text);
                (msg.item_id.clone(), msg.output_index, msg.content_index)
            };
            out.push(OpenAIResponsesStreamEvent::OutputTextDelta {
                item_id,
                output_index,
                content_index,
                delta: text,
                sequence_number: advance_seq(state),
            });
        }
        ChatStreamEvent::ReasoningDelta { .. } => {
            // 本轮忽略 —— Responses 的 reasoning output item 规格尚不稳定，
            // 留给未来细化（应作为独立 output_item type=reasoning 处理）。
            ensure_initialized(&mut out, state, ctx, None);
        }
        ChatStreamEvent::ToolCallDelta(delta) => {
            ensure_initialized(&mut out, state, ctx, None);

            // 切到 tool_call：先关掉打开的 Message
            if state.open_message.is_some() {
                close_open_message(&mut out, state);
            }
            advance_tool_call(&mut out, state, delta);
        }
        ChatStreamEvent::End(end) => {
            ensure_initialized(&mut out, state, ctx, None);

            if state.open_message.is_some() {
                close_open_message(&mut out, state);
            }
            if !state.open_tool_calls.is_empty() {
                close_all_tool_calls(&mut out, state);
            }

            state.final_usage = end.usage;
            state.final_status = finish_reason_to_status(end.finish_reason);
            emit_completed(&mut out, state);
            state.done = true;
        }
        ChatStreamEvent::ThoughtSignature { .. } => {
            // Responses 的 reasoning/thought 规格尚不稳定，保持与 ReasoningDelta 一致忽略。
            ensure_initialized(&mut out, state, ctx, None);
        }
        ChatStreamEvent::UsageDelta(_) => {
            // Responses wire 的 usage 在 `response.completed` 事件里一次性给；
            // 中期 UsageDelta 只给 stream_driver 累计 final_usage。
        }
        ChatStreamEvent::Error(err) => {
            // 透传为 Responses wire 的 `error` 事件（type="error"），客户端据此感知失败。
            // stream_driver 会紧接着终止流并置 Failure outcome。
            ensure_initialized(&mut out, state, ctx, None);
            out.push(OpenAIResponsesStreamEvent::Error {
                code: err.kind,
                message: err.message,
                param: None,
                sequence_number: advance_seq(state),
            });
            state.done = true;
        }
    }
    out
}

// ----- state transitions -----

fn ensure_initialized(
    out: &mut Vec<OpenAIResponsesStreamEvent>,
    state: &mut ResponsesStreamState,
    ctx: &IngressCtx,
    override_model: Option<String>,
) {
    if state.initialized {
        return;
    }
    state.response_id = generate_response_id();
    state.created_at = chrono::Utc::now().timestamp();
    state.model = override_model.unwrap_or_else(|| ctx.actual_model.clone());
    state.final_status = "in_progress".to_string();
    state.initialized = true;

    let snapshot = build_response_snapshot(state);
    out.push(OpenAIResponsesStreamEvent::ResponseCreated {
        response: snapshot.clone(),
        sequence_number: advance_seq(state),
    });
    out.push(OpenAIResponsesStreamEvent::ResponseInProgress {
        response: snapshot,
        sequence_number: advance_seq(state),
    });
}

fn open_new_message(out: &mut Vec<OpenAIResponsesStreamEvent>, state: &mut ResponsesStreamState) {
    let item_id = generate_message_id();
    let output_index = state.next_output_index;
    let content_index = 0u32;

    let item = OpenAIResponsesOutputItem::Message(OpenAIResponsesOutputMessage {
        id: item_id.clone(),
        status: "in_progress".into(),
        role: "assistant".into(),
        content: Vec::new(),
    });
    out.push(OpenAIResponsesStreamEvent::OutputItemAdded {
        output_index,
        item,
        sequence_number: advance_seq(state),
    });

    let part = OpenAIResponsesOutputContentPart::OutputText {
        text: String::new(),
        annotations: Vec::new(),
    };
    out.push(OpenAIResponsesStreamEvent::ContentPartAdded {
        item_id: item_id.clone(),
        output_index,
        content_index,
        part,
        sequence_number: advance_seq(state),
    });

    state.open_message = Some(OpenMessageState {
        item_id,
        output_index,
        content_index,
        text_buf: String::new(),
    });
}

fn close_open_message(out: &mut Vec<OpenAIResponsesStreamEvent>, state: &mut ResponsesStreamState) {
    let msg = match state.open_message.take() {
        Some(m) => m,
        None => return,
    };
    let final_text = msg.text_buf;

    out.push(OpenAIResponsesStreamEvent::OutputTextDone {
        item_id: msg.item_id.clone(),
        output_index: msg.output_index,
        content_index: msg.content_index,
        text: final_text.clone(),
        sequence_number: advance_seq(state),
    });

    let final_part = OpenAIResponsesOutputContentPart::OutputText {
        text: final_text.clone(),
        annotations: Vec::new(),
    };
    out.push(OpenAIResponsesStreamEvent::ContentPartDone {
        item_id: msg.item_id.clone(),
        output_index: msg.output_index,
        content_index: msg.content_index,
        part: final_part.clone(),
        sequence_number: advance_seq(state),
    });

    let final_item = OpenAIResponsesOutputItem::Message(OpenAIResponsesOutputMessage {
        id: msg.item_id,
        status: "completed".into(),
        role: "assistant".into(),
        content: vec![final_part],
    });
    out.push(OpenAIResponsesStreamEvent::OutputItemDone {
        output_index: msg.output_index,
        item: final_item.clone(),
        sequence_number: advance_seq(state),
    });

    state.completed_items.push(final_item);
    state.next_output_index += 1;
}

fn advance_tool_call(
    out: &mut Vec<OpenAIResponsesStreamEvent>,
    state: &mut ResponsesStreamState,
    delta: ToolCallDelta,
) {
    let canon_idx = delta.index;

    // 1) 首次见到这个 canonical index：建 state，但先不 emit output_item.added
    //    —— 上游常把首个 ToolCallDelta 拆多块发（先 id、后 name），name 未就位前
    //    发 added 会给客户端写进带空 name 的 item，随后被客户端原样回传上游
    //    触发 `Invalid 'input[*].name': empty string`。对齐 rust-genai 的
    //    `capture_tool_call` 策略：任意 delta 非空就 merge，name 就位才开口。
    if !state.open_tool_calls.contains_key(&canon_idx) {
        let item_id = generate_fc_id();
        let output_index = state.next_output_index;
        state.next_output_index += 1;
        let call_id = delta
            .id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("call_{}", item_id));
        let name = delta.name.clone().unwrap_or_default();

        state.open_tool_calls.insert(
            canon_idx,
            OpenToolCallState {
                item_id,
                output_index,
                call_id,
                name,
                arguments: String::new(),
                item_added_emitted: false,
                pending_arguments: String::new(),
            },
        );
    }

    // 2) 每次 delta：合并非空的 call_id / name 到 state（rust-genai 做法）
    {
        let tc = state
            .open_tool_calls
            .get_mut(&canon_idx)
            .expect("just inserted above");
        if let Some(new_id) = delta.id.as_deref().filter(|s| !s.is_empty()) {
            tc.call_id = new_id.to_string();
        }
        if let Some(new_name) = delta.name.as_deref().filter(|s| !s.is_empty()) {
            tc.name = new_name.to_string();
        }
    }

    // 3) 把 arguments_delta 先放 pending，真正 emit 要等 added 发出之后
    if let Some(args_delta) = delta.arguments_delta.filter(|s| !s.is_empty()) {
        state
            .open_tool_calls
            .get_mut(&canon_idx)
            .expect("inserted")
            .pending_arguments
            .push_str(&args_delta);
    }

    // 4) 若 name 就位且还没发 added：补发 added + 一次性 flush pending arguments
    flush_pending_tool_call(out, state, canon_idx);
}

/// 尝试为指定 tool_call 补发 `output_item.added`（若 name 就位且未发过）+ flush
/// 缓存的 arguments。name 仍空时维持缓存不动。
fn flush_pending_tool_call(
    out: &mut Vec<OpenAIResponsesStreamEvent>,
    state: &mut ResponsesStreamState,
    canon_idx: i32,
) {
    let Some(tc) = state.open_tool_calls.get(&canon_idx) else {
        return;
    };
    if tc.item_added_emitted {
        // 已发过 added，直接把增量发出去即可
        let pending = std::mem::take(
            &mut state
                .open_tool_calls
                .get_mut(&canon_idx)
                .expect("present")
                .pending_arguments,
        );
        if !pending.is_empty() {
            let tc = state.open_tool_calls.get_mut(&canon_idx).expect("present");
            tc.arguments.push_str(&pending);
            out.push(OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta {
                item_id: tc.item_id.clone(),
                output_index: tc.output_index,
                delta: pending,
                sequence_number: advance_seq(state),
            });
        }
        return;
    }
    if tc.name.is_empty() {
        return; // 还在等上游补 name
    }

    // name 就位：发 added，然后 flush pending
    let (item_id, output_index, call_id, name) = (
        tc.item_id.clone(),
        tc.output_index,
        tc.call_id.clone(),
        tc.name.clone(),
    );
    let item = OpenAIResponsesOutputItem::FunctionCall(OpenAIResponsesOutputFunctionCall {
        id: item_id.clone(),
        call_id,
        name,
        arguments: String::new(),
        status: Some("in_progress".into()),
    });
    out.push(OpenAIResponsesStreamEvent::OutputItemAdded {
        output_index,
        item,
        sequence_number: advance_seq(state),
    });
    state
        .open_tool_calls
        .get_mut(&canon_idx)
        .expect("present")
        .item_added_emitted = true;

    let pending = std::mem::take(
        &mut state
            .open_tool_calls
            .get_mut(&canon_idx)
            .expect("present")
            .pending_arguments,
    );
    if !pending.is_empty() {
        let tc = state.open_tool_calls.get_mut(&canon_idx).expect("present");
        tc.arguments.push_str(&pending);
        out.push(OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta {
            item_id,
            output_index,
            delta: pending,
            sequence_number: advance_seq(state),
        });
    }
}

fn close_all_tool_calls(
    out: &mut Vec<OpenAIResponsesStreamEvent>,
    state: &mut ResponsesStreamState,
) {
    // 1) 兜底：若到关闭时还有 tool_call 没发 output_item.added（说明上游从头到尾
    //    没给 name），warn + 用 "unknown_function" 顶上，保证 stream 能正常结束。
    //    收集所有需要补 name 的 canonical_idx，避免一边迭代一边借用冲突。
    let idxs_need_fallback: Vec<i32> = state
        .open_tool_calls
        .iter()
        .filter_map(|(idx, tc)| (!tc.item_added_emitted && tc.name.is_empty()).then_some(*idx))
        .collect();
    for idx in idxs_need_fallback {
        if let Some(tc) = state.open_tool_calls.get_mut(&idx) {
            tracing::warn!(
                call_id = %tc.call_id,
                "upstream tool_call never provided function name; falling back to \"unknown_function\""
            );
            tc.name = "unknown_function".into();
        }
        flush_pending_tool_call(out, state, idx);
    }

    let tool_calls = std::mem::take(&mut state.open_tool_calls);
    let mut entries: Vec<OpenToolCallState> = tool_calls.into_values().collect();
    entries.sort_by_key(|t| t.output_index);

    for tc in entries {
        // 2) 万一 flush 之后依然没发 added（理论不发生），再保险兜一次
        if !tc.item_added_emitted {
            let item = OpenAIResponsesOutputItem::FunctionCall(OpenAIResponsesOutputFunctionCall {
                id: tc.item_id.clone(),
                call_id: tc.call_id.clone(),
                name: tc.name.clone(),
                arguments: String::new(),
                status: Some("in_progress".into()),
            });
            out.push(OpenAIResponsesStreamEvent::OutputItemAdded {
                output_index: tc.output_index,
                item,
                sequence_number: advance_seq(state),
            });
            if !tc.pending_arguments.is_empty() {
                out.push(OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta {
                    item_id: tc.item_id.clone(),
                    output_index: tc.output_index,
                    delta: tc.pending_arguments.clone(),
                    sequence_number: advance_seq(state),
                });
            }
        }

        out.push(OpenAIResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: tc.item_id.clone(),
            output_index: tc.output_index,
            arguments: tc.arguments.clone(),
            sequence_number: advance_seq(state),
        });

        let final_item =
            OpenAIResponsesOutputItem::FunctionCall(OpenAIResponsesOutputFunctionCall {
                id: tc.item_id,
                call_id: tc.call_id,
                name: tc.name,
                arguments: tc.arguments,
                status: Some("completed".into()),
            });
        out.push(OpenAIResponsesStreamEvent::OutputItemDone {
            output_index: tc.output_index,
            item: final_item.clone(),
            sequence_number: advance_seq(state),
        });

        state.completed_items.push(final_item);
    }
}

fn emit_completed(out: &mut Vec<OpenAIResponsesStreamEvent>, state: &mut ResponsesStreamState) {
    if state.final_status == "in_progress" {
        state.final_status = "completed".into();
    }
    let snapshot = build_response_snapshot(state);
    out.push(OpenAIResponsesStreamEvent::ResponseCompleted {
        response: snapshot,
        sequence_number: advance_seq(state),
    });
}

// ----- helpers -----

fn build_response_snapshot(state: &ResponsesStreamState) -> OpenAIResponsesResponse {
    let output = state.completed_items.clone();
    let output_text = collect_output_text(&output);
    OpenAIResponsesResponse {
        id: state.response_id.clone(),
        object: "response".into(),
        created_at: state.created_at,
        model: state.model.clone(),
        status: state.final_status.clone(),
        output,
        usage: state.final_usage.as_ref().map(usage_to_responses_usage),
        output_text,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        temperature: None,
        top_p: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        tool_choice: None,
        tools: Vec::new(),
        reasoning: None,
        user: None,
        metadata: None,
        error: None,
    }
}

fn usage_to_responses_usage(u: &Usage) -> OpenAIResponsesUsage {
    OpenAIResponsesUsage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        input_tokens_details: None,
        output_tokens_details: None,
    }
}

fn collect_output_text(items: &[OpenAIResponsesOutputItem]) -> Option<String> {
    let mut buf = String::new();
    for item in items {
        if let OpenAIResponsesOutputItem::Message(m) = item {
            for part in &m.content {
                if let OpenAIResponsesOutputContentPart::OutputText { text, .. } = part {
                    buf.push_str(text);
                }
            }
        }
    }
    if buf.is_empty() { None } else { Some(buf) }
}

fn finish_reason_to_status(reason: Option<FinishReason>) -> String {
    match reason {
        Some(FinishReason::Stop)
        | Some(FinishReason::ToolCalls)
        | Some(FinishReason::FunctionCall)
        | None => "completed".into(),
        Some(FinishReason::Length) | Some(FinishReason::ContentFilter) => "incomplete".into(),
    }
}

fn advance_seq(state: &mut ResponsesStreamState) -> u64 {
    let n = state.sequence_number;
    state.sequence_number = state.sequence_number.saturating_add(1);
    n
}

fn generate_response_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("resp_{}", short_id(&COUNTER))
}

fn generate_message_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("msg_{}", short_id(&COUNTER))
}

fn generate_fc_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("fc_{}", short_id(&COUNTER))
}

fn short_id(counter: &AtomicU64) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = counter.fetch_add(1, Ordering::Relaxed);
    format!("{ts:x}{seq:x}")
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::{AdapterKind, ChatChoice, Role, Usage};

    fn ctx() -> IngressCtx {
        IngressCtx::new(AdapterKind::OpenAI, "gpt-5", "gpt-5")
    }

    // -------- to_canonical --------

    #[test]
    fn to_canonical_string_input_single_user_message() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hello"
        }))
        .unwrap();
        let canonical = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.messages.len(), 1);
        assert!(matches!(canonical.messages[0].role, Role::User));
        assert_eq!(canonical.messages[0].text(), Some("hello"));
    }

    #[test]
    fn to_canonical_instructions_becomes_system_message() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hi",
            "instructions": "be concise"
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.messages.len(), 2);
        assert!(matches!(c.messages[0].role, Role::System));
        assert_eq!(c.messages[0].text(), Some("be concise"));
        assert!(matches!(c.messages[1].role, Role::User));
    }

    #[test]
    fn to_canonical_items_message_and_function_call_output() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": [
                {"type":"message","role":"user","content":"what's the weather?"},
                {"type":"function_call","call_id":"call_1","name":"get_weather","arguments":"{}"},
                {"type":"function_call_output","call_id":"call_1","output":"sunny"}
            ]
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.messages.len(), 3);
        assert!(matches!(c.messages[0].role, Role::User));
        assert!(matches!(c.messages[1].role, Role::Assistant));
        assert!(c.messages[1].tool_calls.is_some());
        assert!(matches!(c.messages[2].role, Role::Tool));
        assert_eq!(c.messages[2].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(c.messages[2].text(), Some("sunny"));
    }

    #[test]
    fn to_canonical_message_with_image_part() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": [{
                "type":"message",
                "role":"user",
                "content":[
                    {"type":"input_text","text":"describe"},
                    {"type":"input_image","image_url":"https://x.com/a.png","detail":"high"}
                ]
            }]
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        let m = &c.messages[0];
        let content = m.content.as_ref().unwrap();
        match content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(parts[0], ContentPart::Text { .. }));
                assert!(matches!(parts[1], ContentPart::ImageUrl { .. }));
            }
            _ => panic!("expected Parts"),
        }
    }

    #[test]
    fn to_canonical_function_tool_maps() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hi",
            "tools": [{
                "type":"function",
                "name":"get_weather",
                "description":"get weather",
                "parameters": {"type":"object","properties":{}}
            }]
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        let tools = c.tools.expect("tools should be present");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.as_ref().unwrap().name, "get_weather");
    }

    #[test]
    fn to_canonical_builtin_tool_preserved() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hi",
            "tools": [{
                "type":"web_search_preview",
                "search_context_size":"medium"
            }]
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        let tools = c.tools.expect("tools should be present");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].kind, "web_search_preview");
        assert!(tools[0].function.is_none());
        assert_eq!(
            tools[0].extra["search_context_size"],
            serde_json::json!("medium")
        );
    }

    #[test]
    fn to_canonical_previous_response_id_reasoning_summary_and_instructions_go_to_responses_extras()
    {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hi",
            "instructions": "be concise",
            "previous_response_id": "resp_prev",
            "reasoning": {
                "effort": "high",
                "summary": "auto"
            },
            "store": true
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        let extras = c.responses_extras.expect("responses_extras should exist");
        assert_eq!(extras.instructions.as_deref(), Some("be concise"));
        assert_eq!(extras.previous_response_id.as_deref(), Some("resp_prev"));
        assert_eq!(extras.reasoning_summary.as_deref(), Some("auto"));
        assert_eq!(
            c.reasoning_effort,
            Some(summer_ai_core::ReasoningEffort::High)
        );
        assert_eq!(c.store, Some(true));
        assert!(matches!(c.messages[0].role, Role::System));
        assert_eq!(c.messages[0].text(), Some("be concise"));
    }

    #[test]
    fn to_canonical_max_output_tokens_to_max_completion_tokens() {
        let req: OpenAIResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5",
            "input": "hi",
            "max_output_tokens": 256
        }))
        .unwrap();
        let c = OpenAIResponsesIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.max_completion_tokens, Some(256));
    }

    // -------- from_canonical --------

    fn chat_response(text: &str, finish: FinishReason) -> ChatResponse {
        ChatResponse {
            id: "chatcmpl-1".into(),
            object: "chat.completion".into(),
            created: 1000,
            model: "gpt-5".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage::assistant(text),
                logprobs: None,
                finish_reason: Some(finish),
            }],
            usage: Usage {
                prompt_tokens: 3,
                completion_tokens: 2,
                total_tokens: 5,
                ..Default::default()
            },
            system_fingerprint: None,
            service_tier: None,
        }
    }

    #[test]
    fn from_canonical_text_produces_message_output_and_output_text() {
        let resp = chat_response("hi", FinishReason::Stop);
        let out = OpenAIResponsesIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(out.status, "completed");
        assert_eq!(out.output.len(), 1);
        assert_eq!(out.output_text.as_deref(), Some("hi"));
        let msg = match &out.output[0] {
            OpenAIResponsesOutputItem::Message(m) => m,
            _ => panic!("expected Message"),
        };
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            OpenAIResponsesOutputContentPart::OutputText { text, .. } => {
                assert_eq!(text, "hi");
            }
            _ => panic!("expected OutputText"),
        }
        let usage = out.usage.unwrap();
        assert_eq!(usage.input_tokens, 3);
        assert_eq!(usage.output_tokens, 2);
        assert_eq!(usage.total_tokens, 5);
    }

    #[test]
    fn from_canonical_length_finish_becomes_incomplete() {
        let resp = chat_response("truncated", FinishReason::Length);
        let out = OpenAIResponsesIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(out.status, "incomplete");
    }

    #[test]
    fn from_canonical_tool_calls_become_function_call_output_items() {
        let mut resp = chat_response("", FinishReason::ToolCalls);
        resp.choices[0].message.content = None;
        resp.choices[0].message.tool_calls = Some(vec![ToolCall {
            id: "call_1".into(),
            kind: "function".into(),
            function: ToolCallFunction {
                name: "get_weather".into(),
                arguments: "{}".into(),
            },
            thought_signatures: None,
        }]);
        let out = OpenAIResponsesIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(out.output.len(), 1);
        let fc = match &out.output[0] {
            OpenAIResponsesOutputItem::FunctionCall(f) => f,
            _ => panic!("expected FunctionCall"),
        };
        assert_eq!(fc.call_id, "call_1");
        assert_eq!(fc.name, "get_weather");
    }

    #[test]
    fn from_canonical_refusal_becomes_refusal_part() {
        let mut resp = chat_response("", FinishReason::Stop);
        resp.choices[0].message.content = None;
        resp.choices[0].message.refusal = Some("nope".into());
        let out = OpenAIResponsesIngress::from_canonical(resp, &ctx()).unwrap();
        let msg = match &out.output[0] {
            OpenAIResponsesOutputItem::Message(m) => m,
            _ => panic!("expected Message"),
        };
        match &msg.content[0] {
            OpenAIResponsesOutputContentPart::Refusal { refusal } => {
                assert_eq!(refusal, "nope");
            }
            _ => panic!("expected Refusal"),
        }
    }

    // -------- stream: helpers --------

    fn new_state() -> StreamConvertState {
        StreamConvertState::for_format(IngressFormat::OpenAIResponses)
    }

    fn emit(
        state: &mut StreamConvertState,
        event: ChatStreamEvent,
    ) -> Vec<OpenAIResponsesStreamEvent> {
        OpenAIResponsesIngress::from_canonical_stream_event(event, state, &ctx()).unwrap()
    }

    fn start_event() -> ChatStreamEvent {
        ChatStreamEvent::Start {
            adapter: "openai".into(),
            model: "gpt-5".into(),
        }
    }

    fn end_event(reason: FinishReason, usage: Option<summer_ai_core::Usage>) -> ChatStreamEvent {
        ChatStreamEvent::End(summer_ai_core::StreamEnd {
            finish_reason: Some(reason),
            usage,
        })
    }

    fn seq_of(ev: &OpenAIResponsesStreamEvent) -> u64 {
        match ev {
            OpenAIResponsesStreamEvent::ResponseCreated {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ResponseInProgress {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::OutputItemAdded {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ContentPartAdded {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::OutputTextDelta {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::OutputTextDone {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ContentPartDone {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::OutputItemDone {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::FunctionCallArgumentsDone {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ResponseCompleted {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ResponseFailed {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::ResponseIncomplete {
                sequence_number, ..
            }
            | OpenAIResponsesStreamEvent::Error {
                sequence_number, ..
            } => *sequence_number,
        }
    }

    // -------- stream: text-only path --------

    #[test]
    fn stream_start_emits_created_and_in_progress() {
        let mut state = new_state();
        let events = emit(&mut state, start_event());
        assert_eq!(events.len(), 2);
        matches!(
            events[0],
            OpenAIResponsesStreamEvent::ResponseCreated { .. }
        );
        matches!(
            events[1],
            OpenAIResponsesStreamEvent::ResponseInProgress { .. }
        );
        assert_eq!(seq_of(&events[0]), 0);
        assert_eq!(seq_of(&events[1]), 1);
    }

    #[test]
    fn stream_text_flow_produces_canonical_event_ladder() {
        let mut state = new_state();
        let mut all: Vec<OpenAIResponsesStreamEvent> = Vec::new();
        all.extend(emit(&mut state, start_event()));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::TextDelta {
                text: "Hello".into(),
            },
        ));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::TextDelta {
                text: " world".into(),
            },
        ));
        all.extend(emit(
            &mut state,
            end_event(
                FinishReason::Stop,
                Some(summer_ai_core::Usage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                    total_tokens: 7,
                    ..Default::default()
                }),
            ),
        ));

        // sequence_number 严格递增且从 0 起
        for (i, ev) in all.iter().enumerate() {
            assert_eq!(seq_of(ev), i as u64, "event {i} seq");
        }

        // 预期事件序列
        let expected_order = [
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta", // Hello
            "response.output_text.delta", // world
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ];
        let actual_order: Vec<&'static str> = all
            .iter()
            .map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCreated { .. } => "response.created",
                OpenAIResponsesStreamEvent::ResponseInProgress { .. } => "response.in_progress",
                OpenAIResponsesStreamEvent::OutputItemAdded { .. } => "response.output_item.added",
                OpenAIResponsesStreamEvent::ContentPartAdded { .. } => {
                    "response.content_part.added"
                }
                OpenAIResponsesStreamEvent::OutputTextDelta { .. } => "response.output_text.delta",
                OpenAIResponsesStreamEvent::OutputTextDone { .. } => "response.output_text.done",
                OpenAIResponsesStreamEvent::ContentPartDone { .. } => "response.content_part.done",
                OpenAIResponsesStreamEvent::OutputItemDone { .. } => "response.output_item.done",
                OpenAIResponsesStreamEvent::ResponseCompleted { .. } => "response.completed",
                _ => "other",
            })
            .collect();
        assert_eq!(actual_order, expected_order);

        // output_text.done 的 text 是累积全文
        let done = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputTextDone { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .unwrap();
        assert_eq!(done, "Hello world");

        // completed 里 output_text / usage
        let completed = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCompleted { response, .. } => Some(response),
                _ => None,
            })
            .unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.output_text.as_deref(), Some("Hello world"));
        let usage = completed.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 5);
        assert_eq!(usage.output_tokens, 2);
        assert_eq!(usage.total_tokens, 7);
    }

    // -------- stream: tool call path --------

    #[test]
    fn stream_tool_call_single_produces_function_call_item_pair() {
        let mut state = new_state();
        let mut all = Vec::new();
        all.extend(emit(&mut state, start_event()));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("call_1".into()),
                name: Some("get_weather".into()),
                arguments_delta: Some("{\"loc".into()),
            }),
        ));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments_delta: Some("\":\"NYC\"}".into()),
            }),
        ));
        all.extend(emit(&mut state, end_event(FinishReason::ToolCalls, None)));

        // 期望：OutputItemAdded(function_call) + FunctionCallArgumentsDelta×2
        // + FunctionCallArgumentsDone + OutputItemDone + ResponseCompleted
        let kinds: Vec<&'static str> = all
            .iter()
            .skip(2) // 跳过 created + in_progress
            .map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputItemAdded { .. } => "added",
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta { .. } => "fc.delta",
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDone { .. } => "fc.done",
                OpenAIResponsesStreamEvent::OutputItemDone { .. } => "item.done",
                OpenAIResponsesStreamEvent::ResponseCompleted { .. } => "completed",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "added",
                "fc.delta",
                "fc.delta",
                "fc.done",
                "item.done",
                "completed"
            ]
        );

        // fc.done 的 arguments 是累积完整串
        let done_args = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDone { arguments, .. } => {
                    Some(arguments.as_str())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(done_args, "{\"loc\":\"NYC\"}");

        // completed 的 output 里含 FunctionCall item
        let completed = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCompleted { response, .. } => Some(response),
                _ => None,
            })
            .unwrap();
        assert_eq!(completed.output.len(), 1);
        let fc = match &completed.output[0] {
            OpenAIResponsesOutputItem::FunctionCall(f) => f,
            _ => panic!("expected FunctionCall"),
        };
        assert_eq!(fc.call_id, "call_1");
        assert_eq!(fc.name, "get_weather");
        assert_eq!(fc.arguments, "{\"loc\":\"NYC\"}");
    }

    #[test]
    fn stream_multiple_tool_calls_share_different_output_indexes() {
        let mut state = new_state();
        let _ = emit(&mut state, start_event());
        let _ = emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("call_a".into()),
                name: Some("fa".into()),
                arguments_delta: Some("{}".into()),
            }),
        );
        let _ = emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 1,
                id: Some("call_b".into()),
                name: Some("fb".into()),
                arguments_delta: Some("{}".into()),
            }),
        );
        let tail = emit(&mut state, end_event(FinishReason::ToolCalls, None));

        let completed = tail
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCompleted { response, .. } => Some(response),
                _ => None,
            })
            .unwrap();
        assert_eq!(completed.output.len(), 2);
    }

    #[test]
    fn stream_tool_call_defers_output_item_added_until_name_arrives() {
        // 上游首块只给 id（name 空）、后续块才把 name 塞进来（hybgzs / 某些聚合网关
        // 的拆分模式）。Responses API 的 `output_item.added` 事件里 item.name 必须
        // 已定稿，所以 added 事件要延后到 name 就位，且同时 flush 缓存的 arguments。
        let mut state = new_state();
        let mut all = Vec::new();
        all.extend(emit(&mut state, start_event()));

        // 首 delta：只带 id，name None，arguments "a"
        all.extend(emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("call_x".into()),
                name: None,
                arguments_delta: Some("{\"a\"".into()),
            }),
        ));

        // 首 delta 之后不应该已经有 OutputItemAdded
        assert!(
            !all.iter()
                .any(|ev| matches!(ev, OpenAIResponsesStreamEvent::OutputItemAdded { .. })),
            "output_item.added must not fire while name is still empty"
        );

        // 第二 delta：带上 name，arguments 继续增量
        all.extend(emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: None,
                name: Some("shell".into()),
                arguments_delta: Some(":1}".into()),
            }),
        ));
        all.extend(emit(&mut state, end_event(FinishReason::ToolCalls, None)));

        // Added 在 name 就位那一刻 emit，且带上确定的 name
        let added = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputItemAdded { item, .. } => match item {
                    OpenAIResponsesOutputItem::FunctionCall(f) => Some(f),
                    _ => None,
                },
                _ => None,
            })
            .expect("expected OutputItemAdded after name arrived");
        assert_eq!(added.call_id, "call_x");
        assert_eq!(added.name, "shell");

        // name 就位前缓存的 arguments（"{\"a\""）和触发 flush 那一刻新追加的
        // ":1}" 会合并成一条 `function_call_arguments.delta` —— 因为 added 未发
        // 时所有增量都堆在 pending 里，flush 时一次性吐出。
        let deltas: Vec<&str> = all
            .iter()
            .filter_map(|ev| match ev {
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta { delta, .. } => {
                    Some(delta.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["{\"a\":1}"]);

        // Done 的累积 arguments 完整
        let done = all
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDone { arguments, .. } => {
                    Some(arguments.as_str())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(done, "{\"a\":1}");
    }

    #[test]
    fn stream_tool_call_merges_non_empty_name_from_later_delta() {
        // 同 index 后续 delta 带 name 时，即使首 delta 已经发了 added（本用例未发，
        // 因为首 delta 也带空 name），state.name 最终要以最后非空 delta 为准。
        let mut state = new_state();
        let _ = emit(&mut state, start_event());
        let _ = emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("call_y".into()),
                name: Some("".into()),
                arguments_delta: None,
            }),
        );
        let _ = emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: None,
                name: Some("real_fn".into()),
                arguments_delta: Some("{}".into()),
            }),
        );
        let tail = emit(&mut state, end_event(FinishReason::ToolCalls, None));

        let fc = tail
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputItemDone { item, .. } => match item {
                    OpenAIResponsesOutputItem::FunctionCall(f) => Some(f),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_eq!(fc.name, "real_fn");
    }

    #[test]
    fn stream_tool_call_falls_back_to_unknown_function_when_name_never_arrives() {
        // 上游从头到尾没给 name（异常情况）——close 时兜底 "unknown_function"，保证
        // `output_item.added` 和 `output_item.done` 仍然发，客户端不会卡住。
        let mut state = new_state();
        let _ = emit(&mut state, start_event());
        let _ = emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("call_z".into()),
                name: None,
                arguments_delta: Some("{}".into()),
            }),
        );
        let tail = emit(&mut state, end_event(FinishReason::ToolCalls, None));

        let fc = tail
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputItemDone { item, .. } => match item {
                    OpenAIResponsesOutputItem::FunctionCall(f) => Some(f),
                    _ => None,
                },
                _ => None,
            })
            .expect("expected OutputItemDone fallback");
        assert_eq!(fc.name, "unknown_function");
        assert_eq!(fc.call_id, "call_z");
        assert_eq!(fc.arguments, "{}");
    }

    // -------- stream: mixed text → tool_call path --------

    #[test]
    fn stream_text_then_tool_call_closes_message_before_opening_function_call() {
        let mut state = new_state();
        let mut all = Vec::new();
        all.extend(emit(&mut state, start_event()));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::TextDelta {
                text: "Thinking...".into(),
            },
        ));
        all.extend(emit(
            &mut state,
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 0,
                id: Some("c1".into()),
                name: Some("tool".into()),
                arguments_delta: Some("{}".into()),
            }),
        ));
        all.extend(emit(&mut state, end_event(FinishReason::ToolCalls, None)));

        // 序列：created, in_progress, msg_added, part_added, text.delta, text.done,
        // part.done, msg.done, fc_added, fc.delta, fc.done, fc_item.done, completed
        let kinds: Vec<&'static str> = all
            .iter()
            .map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCreated { .. } => "created",
                OpenAIResponsesStreamEvent::ResponseInProgress { .. } => "in_progress",
                OpenAIResponsesStreamEvent::OutputItemAdded { item, .. } => match item {
                    OpenAIResponsesOutputItem::Message(_) => "msg_added",
                    OpenAIResponsesOutputItem::FunctionCall(_) => "fc_added",
                    _ => "unknown_added",
                },
                OpenAIResponsesStreamEvent::ContentPartAdded { .. } => "part_added",
                OpenAIResponsesStreamEvent::OutputTextDelta { .. } => "text.delta",
                OpenAIResponsesStreamEvent::OutputTextDone { .. } => "text.done",
                OpenAIResponsesStreamEvent::ContentPartDone { .. } => "part.done",
                OpenAIResponsesStreamEvent::OutputItemDone { item, .. } => match item {
                    OpenAIResponsesOutputItem::Message(_) => "msg_item.done",
                    OpenAIResponsesOutputItem::FunctionCall(_) => "fc_item.done",
                    _ => "unknown_item.done",
                },
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta { .. } => "fc.delta",
                OpenAIResponsesStreamEvent::FunctionCallArgumentsDone { .. } => "fc.done",
                OpenAIResponsesStreamEvent::ResponseCompleted { .. } => "completed",
                _ => "other",
            })
            .collect();

        assert_eq!(
            kinds,
            vec![
                "created",
                "in_progress",
                "msg_added",
                "part_added",
                "text.delta",
                "text.done",
                "part.done",
                "msg_item.done",
                "fc_added",
                "fc.delta",
                "fc.done",
                "fc_item.done",
                "completed",
            ]
        );

        // message 的 output_index = 0；fc 的 output_index = 1
        let output_indexes: Vec<u32> = all
            .iter()
            .filter_map(|ev| match ev {
                OpenAIResponsesStreamEvent::OutputItemDone {
                    item, output_index, ..
                } => Some(match item {
                    OpenAIResponsesOutputItem::Message(_) => ("msg", *output_index),
                    OpenAIResponsesOutputItem::FunctionCall(_) => ("fc", *output_index),
                    _ => ("unknown", *output_index),
                }),
                _ => None,
            })
            .map(|(_, idx)| idx)
            .collect();
        assert_eq!(output_indexes, vec![0, 1]);
    }

    // -------- stream: reasoning is ignored --------

    #[test]
    fn stream_reasoning_delta_is_dropped_but_initialization_still_happens() {
        let mut state = new_state();
        let events = emit(
            &mut state,
            ChatStreamEvent::ReasoningDelta {
                text: "thinking".into(),
            },
        );
        // 只有 created + in_progress，没有 text.delta
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|ev| !matches!(ev, OpenAIResponsesStreamEvent::OutputTextDelta { .. }))
        );
    }

    // -------- stream: length finish maps to incomplete --------

    #[test]
    fn stream_end_with_length_maps_to_incomplete_status() {
        let mut state = new_state();
        let _ = emit(&mut state, start_event());
        let _ = emit(&mut state, ChatStreamEvent::TextDelta { text: "t".into() });
        let tail = emit(&mut state, end_event(FinishReason::Length, None));
        let completed = tail
            .iter()
            .find_map(|ev| match ev {
                OpenAIResponsesStreamEvent::ResponseCompleted { response, .. } => Some(response),
                _ => None,
            })
            .unwrap();
        assert_eq!(completed.status, "incomplete");
    }

    // -------- stream: wrong state variant --------

    #[test]
    fn stream_wrong_state_variant_returns_unsupported() {
        let mut state = StreamConvertState::for_format(IngressFormat::Claude);
        let err = OpenAIResponsesIngress::from_canonical_stream_event(
            ChatStreamEvent::TextDelta { text: "x".into() },
            &mut state,
            &ctx(),
        )
        .unwrap_err();
        assert!(matches!(err, AdapterError::Unsupported { .. }));
    }

    // -------- stream: after End, further events are no-op --------

    #[test]
    fn stream_events_after_end_are_dropped() {
        let mut state = new_state();
        let _ = emit(&mut state, start_event());
        let _ = emit(&mut state, end_event(FinishReason::Stop, None));
        let more = emit(
            &mut state,
            ChatStreamEvent::TextDelta {
                text: "late".into(),
            },
        );
        assert!(more.is_empty());
    }
}
