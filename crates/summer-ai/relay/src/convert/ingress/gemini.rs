//! Google Gemini GenerateContent ↔ canonical 转换。
//!
//! # 流式
//!
//! Gemini 流不是 SSE 事件序列，而是**多个完整的 `GeminiChatResponse` JSON 对象**
//! 拼接（类似 NDJSON）。所以每个 canonical 流事件被独立序列化成一个完整 response。
//!
//! # 已知限制
//!
//! 1. **`thinkingConfig` 仅路由层透传到 extra**：canonical `reasoning_effort` 是
//!    `"low"/"medium"/"high"` 字符串，跟 `thinking_budget` 数字映射关系不直接，
//!    当前用简单阈值（<=0 None, <=1024 low, <=4096 medium, >4096 high）。
//! 2. **`safetySettings` 透传到 extra**：canonical 没有对应字段。
//! 3. **`grounding` / `codeExecution` 工具** 在 tool 列表里以 `serde_json::Value`
//!    透传（不是 functionDeclarations 的不动）。
//! 4. **`stop_sequences` 限制 4 个**（canonical 上游 OpenAI 只接受 ≤4，Gemini 最多 5 个）。
//! 5. **流式 `ToolCallDelta` 暂不产出 functionCall chunks**：Gemini 流式工具调用
//!    的增量表达 pending（需要额外状态累积 args），当前文本流 + 结束事件可用。

use summer_ai_core::types::ingress_wire::gemini::{
    GeminiCandidate, GeminiChatResponse, GeminiContent, GeminiFileData, GeminiFunctionCall,
    GeminiFunctionCallingConfig, GeminiFunctionResponse, GeminiGenerateContentRequest,
    GeminiGenerationConfig, GeminiInlineData, GeminiPart, GeminiTool, GeminiToolConfig,
    GeminiUsageMetadata,
};
use summer_ai_core::{
    AdapterError, AdapterResult, ChatChoice, ChatMessage, ChatRequest, ChatResponse,
    ChatStreamEvent, ContentPart, FinishReason, ImageUrl, MessageContent, Role, Tool, ToolCall,
    ToolCallFunction, ToolChoice, ToolFunction, Usage,
};

use super::{GeminiStreamState, IngressConverter, IngressCtx, IngressFormat, StreamConvertState};

/// Gemini GenerateContent 入口协议转换器。
pub struct GeminiIngress;

impl IngressConverter for GeminiIngress {
    type ClientRequest = GeminiGenerateContentRequest;
    type ClientResponse = GeminiChatResponse;
    type ClientStreamEvent = GeminiChatResponse;

    const FORMAT: IngressFormat = IngressFormat::Gemini;

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
        let StreamConvertState::Gemini(gem_state) = state else {
            return Err(AdapterError::Unsupported {
                adapter: "gemini_ingress",
                feature: "stream_convert_state_mismatch",
            });
        };
        Ok(from_canonical_stream_event_impl(event, gem_state, ctx))
    }
}

// ---------------------------------------------------------------------------
// to_canonical
// ---------------------------------------------------------------------------

fn to_canonical_impl(
    req: GeminiGenerateContentRequest,
    _ctx: &IngressCtx,
) -> AdapterResult<ChatRequest> {
    let GeminiGenerateContentRequest {
        contents,
        system_instruction,
        tools,
        tool_config,
        generation_config,
        safety_settings,
        extra: _,
    } = req;

    let mut canonical_messages: Vec<ChatMessage> = Vec::new();

    // systemInstruction → system message
    if let Some(sys) = system_instruction {
        let text = sys
            .parts
            .into_iter()
            .filter_map(extract_text_from_part)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            canonical_messages.push(ChatMessage::system(text));
        }
    }

    // 建立顺序生成的 call_id 队列（Gemini 没有 tool call id，按出现顺序编号）
    let mut call_counter: u32 = 0;
    let mut pending_function_call_ids: Vec<String> = Vec::new();
    // 第一遍扫 functionCall 出现顺序 → pending ids
    for content in &contents {
        for part in &content.parts {
            if matches!(part, GeminiPart::FunctionCall { .. }) {
                call_counter += 1;
                pending_function_call_ids.push(format!("call_{call_counter}"));
            }
        }
    }

    let mut fc_id_iter = pending_function_call_ids.iter().cloned();
    // functionResponse 对应的 call_id（按出现顺序配对 functionCall）
    let mut fr_id_iter = pending_function_call_ids.iter().cloned();

    for content in contents {
        append_gemini_content(
            content,
            &mut canonical_messages,
            &mut fc_id_iter,
            &mut fr_id_iter,
        )?;
    }

    // tools
    let canonical_tools = flatten_tools(tools);

    // tool_choice
    let canonical_tool_choice = tool_config.and_then(tool_config_to_choice);

    // generationConfig
    let (temperature, top_p, top_k, n, max_tokens, stop, reasoning_effort, extra_gen) =
        split_generation_config(generation_config);

    let mut extra = extra_gen;
    if let Some(k) = top_k {
        extra.insert(
            "top_k".to_string(),
            serde_json::Value::Number(serde_json::Number::from(k)),
        );
    }
    if !safety_settings.is_empty() {
        extra.insert(
            "safety_settings".to_string(),
            serde_json::to_value(&safety_settings).map_err(AdapterError::SerializeRequest)?,
        );
    }

    Ok(ChatRequest {
        model: _ctx.actual_model.clone(),
        messages: canonical_messages,
        temperature,
        top_p,
        n,
        stop,
        max_tokens,
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
        stream: false, // path 决定（`:streamGenerateContent` vs `:generateContent`），由 handler 设
        stream_options: None,
        reasoning_effort,
        verbosity: None,
        seed: None,
        service_tier: None,
        user: None,
        metadata: None,
        store: None,
        web_search_options: None,
        responses_extras: None,
        extra,
    })
}

fn append_gemini_content(
    content: GeminiContent,
    out: &mut Vec<ChatMessage>,
    _fc_id_iter: &mut dyn Iterator<Item = String>,
    fr_id_iter: &mut dyn Iterator<Item = String>,
) -> AdapterResult<()> {
    let GeminiContent { role, parts } = content;

    let role_enum = match role.as_deref() {
        Some("user") => Role::User,
        Some("model") => Role::Assistant,
        Some("function") => Role::Tool,
        Some(_) | None => Role::User,
    };

    let mut text_buf = String::new();
    let mut parts_buf: Vec<ContentPart> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    // 把 functionResponse 抽出来做 role:"tool" 消息
    let mut tool_responses: Vec<(String, String)> = Vec::new();

    let mut call_counter_local: u32 = 0;

    for part in parts {
        match part {
            GeminiPart::Text { text, .. } => {
                if parts_buf.is_empty() {
                    if !text_buf.is_empty() {
                        text_buf.push('\n');
                    }
                    text_buf.push_str(&text);
                } else {
                    parts_buf.push(ContentPart::Text { text });
                }
            }
            GeminiPart::InlineData { inline_data } => {
                if !text_buf.is_empty() {
                    parts_buf.push(ContentPart::Text {
                        text: std::mem::take(&mut text_buf),
                    });
                }
                let GeminiInlineData { mime_type, data } = inline_data;
                parts_buf.push(ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: format!("data:{mime_type};base64,{data}"),
                        detail: None,
                    },
                });
            }
            GeminiPart::FileData { file_data } => {
                if !text_buf.is_empty() {
                    parts_buf.push(ContentPart::Text {
                        text: std::mem::take(&mut text_buf),
                    });
                }
                let GeminiFileData { file_uri, .. } = file_data;
                parts_buf.push(ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: file_uri,
                        detail: None,
                    },
                });
            }
            GeminiPart::FunctionCall { function_call } => {
                call_counter_local += 1;
                let GeminiFunctionCall { name, args } = function_call;
                let arguments =
                    serde_json::to_string(&args).map_err(AdapterError::SerializeRequest)?;
                tool_calls.push(ToolCall {
                    id: format!("call_{call_counter_local}"),
                    kind: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                    thought_signatures: None,
                });
            }
            GeminiPart::FunctionResponse { function_response } => {
                let GeminiFunctionResponse { response, .. } = function_response;
                let body =
                    serde_json::to_string(&response).map_err(AdapterError::SerializeRequest)?;
                let call_id = fr_id_iter
                    .next()
                    .unwrap_or_else(|| "call_unknown".to_string());
                tool_responses.push((call_id, body));
            }
            GeminiPart::ThoughtSignature { .. } => {
                // Request 里极少出现独立的 thoughtSignature part（一般是 response 侧），
                // 作为 canonical ChatRequest 时没对应字段可挂，忽略。
            }
            GeminiPart::Other(_) => {
                // 未知 part（executableCode / codeExecutionResult 等）暂时忽略
            }
        }
    }

    // tool_responses 先于当前消息（Gemini user 消息里混的 functionResponse 拆出）
    for (call_id, body) in tool_responses {
        out.push(ChatMessage {
            role: Role::Tool,
            content: Some(MessageContent::Text(body)),
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: Some(call_id),
            reasoning_content: None,
            audio: None,
            options: None,
        });
    }

    let message_content: Option<MessageContent> = if !parts_buf.is_empty() {
        if !text_buf.is_empty() {
            parts_buf.insert(0, ContentPart::Text { text: text_buf });
        }
        Some(MessageContent::Parts(parts_buf))
    } else if !text_buf.is_empty() {
        Some(MessageContent::Text(text_buf))
    } else if !tool_calls.is_empty() {
        None
    } else {
        // 纯 tool_response，已追加 → 不 push 空消息
        return Ok(());
    };

    out.push(ChatMessage {
        role: role_enum,
        content: message_content,
        reasoning_content: None,
        refusal: None,
        name: None,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        tool_call_id: None,
        audio: None,
        options: None,
    });

    Ok(())
}

fn extract_text_from_part(part: GeminiPart) -> Option<String> {
    match part {
        GeminiPart::Text { text, .. } => Some(text),
        _ => None,
    }
}

/// Gemini `tools[]` → canonical `Vec<Tool>`。
///
/// 每个 GeminiTool 是 key-based 平面结构，可以同时携带多个 built-in 字段。展开规则：
///
/// - `function_declarations[]` → canonical function tool（每个 decl 单独一个 Tool）
/// - `google_search` 非空 → canonical Tool `kind: "google_search"`，value 进 extra
/// - `code_execution` 非空 → canonical Tool `kind: "code_execution"`
/// - `extra`（flatten 承载的 `urlContext` / `googleSearchRetrieval` / `grounding`
///   等）→ 每个 key 一个 Tool，kind 使用 camelCase 原样保留，value 平铺到 extra
///
/// 这样路由到其他上游（比如 Claude）时，`canonical_tool_to_claude` 能识别
/// `kind.starts_with("google_search")` 映射成 `web_search_20250305`；回到 Gemini
/// 上游时 `build_gemini_tools` 识别同样的 kind 族重新写入 google_search 字段。
fn flatten_tools(tools: Vec<GeminiTool>) -> Option<Vec<Tool>> {
    let mut out = Vec::new();
    for tool in tools {
        let GeminiTool {
            function_declarations,
            google_search,
            code_execution,
            extra,
        } = tool;

        for decl in function_declarations {
            out.push(Tool {
                kind: "function".to_string(),
                function: Some(ToolFunction {
                    name: decl.name,
                    description: decl.description,
                    parameters: decl.parameters,
                }),
                strict: None,
                extra: serde_json::Map::new(),
            });
        }
        if let Some(gs) = google_search {
            out.push(make_builtin_tool(
                summer_ai_core::types::ingress_wire::gemini::kind_prefix::GOOGLE_SEARCH,
                gs,
            ));
        }
        if let Some(ce) = code_execution {
            out.push(make_builtin_tool(
                summer_ai_core::types::ingress_wire::gemini::kind_prefix::CODE_EXECUTION,
                ce,
            ));
        }
        for (kind, value) in extra {
            // kind 是 Gemini 的 camelCase key（`urlContext` / `googleSearchRetrieval`）
            out.push(make_builtin_tool(&kind, value));
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 构造一个 built-in canonical Tool：value 如果是对象就直接作为 extra，
/// 其他形态（`null` / 字符串）塞到 `extra["_value"]` 以免丢失。
fn make_builtin_tool(kind: &str, value: serde_json::Value) -> Tool {
    let extra = match value {
        serde_json::Value::Object(m) => m,
        serde_json::Value::Null => serde_json::Map::new(),
        other => {
            let mut m = serde_json::Map::new();
            m.insert("_value".to_string(), other);
            m
        }
    };
    Tool {
        kind: kind.to_string(),
        function: None,
        strict: None,
        extra,
    }
}

fn tool_config_to_choice(config: GeminiToolConfig) -> Option<ToolChoice> {
    let fc = config.function_calling_config?;
    let GeminiFunctionCallingConfig {
        mode,
        allowed_function_names,
    } = fc;
    match mode.as_deref() {
        Some("AUTO") | None => Some(ToolChoice::Mode("auto".to_string())),
        Some("ANY") => {
            // ANY + 单个允许函数 → Named function；否则 required
            if allowed_function_names.len() == 1 {
                let name = allowed_function_names.into_iter().next().unwrap();
                Some(ToolChoice::Named(serde_json::json!({
                    "type": "function",
                    "function": { "name": name }
                })))
            } else {
                Some(ToolChoice::Mode("required".to_string()))
            }
        }
        Some("NONE") => Some(ToolChoice::Mode("none".to_string())),
        Some(_) => Some(ToolChoice::Mode("auto".to_string())),
    }
}

/// GenerationConfig 字段拆分到 canonical 各处。
fn split_generation_config(
    config: Option<GeminiGenerationConfig>,
) -> (
    Option<f64>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<serde_json::Value>,
    Option<summer_ai_core::ReasoningEffort>,
    serde_json::Map<String, serde_json::Value>,
) {
    let Some(cfg) = config else {
        return (
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            serde_json::Map::new(),
        );
    };

    let GeminiGenerationConfig {
        temperature,
        top_p,
        top_k,
        candidate_count,
        max_output_tokens,
        stop_sequences,
        response_mime_type,
        response_schema,
        thinking_config,
        extra,
    } = cfg;

    // stop_sequences 截前 4 个
    let stop = match stop_sequences.len() {
        0 => None,
        1 => Some(serde_json::Value::String(
            stop_sequences.into_iter().next().unwrap(),
        )),
        n => {
            let truncated: Vec<_> = stop_sequences.into_iter().take(n.min(4)).collect();
            Some(serde_json::Value::Array(
                truncated
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ))
        }
    };

    // response_format: 当前直接透传到 extra（响应格式结构和 OpenAI 不完全一致）
    let mut extra_out = extra;
    if let Some(m) = response_mime_type {
        extra_out.insert(
            "response_mime_type".to_string(),
            serde_json::Value::String(m),
        );
    }
    if let Some(s) = response_schema {
        extra_out.insert("response_schema".to_string(), s);
    }

    let reasoning_effort = thinking_config.and_then(|tc| match tc.thinking_budget {
        None | Some(0) => None,
        Some(n) if n <= 0 => None,
        Some(n) if n <= 1024 => Some(summer_ai_core::ReasoningEffort::Low),
        Some(n) if n <= 4096 => Some(summer_ai_core::ReasoningEffort::Medium),
        Some(_) => Some(summer_ai_core::ReasoningEffort::High),
    });

    (
        temperature,
        top_p,
        top_k,
        candidate_count,
        max_output_tokens,
        stop,
        reasoning_effort,
        extra_out,
    )
}

// ---------------------------------------------------------------------------
// from_canonical
// ---------------------------------------------------------------------------

fn from_canonical_impl(resp: ChatResponse, _ctx: &IngressCtx) -> AdapterResult<GeminiChatResponse> {
    let ChatResponse {
        choices,
        usage,
        model,
        ..
    } = resp;

    let candidates: Vec<GeminiCandidate> = choices
        .into_iter()
        .map(canonical_choice_to_gemini_candidate)
        .collect::<AdapterResult<Vec<_>>>()?;

    Ok(GeminiChatResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata: Some(usage_to_gemini(&usage)),
        model_version: Some(model),
    })
}

fn canonical_choice_to_gemini_candidate(choice: ChatChoice) -> AdapterResult<GeminiCandidate> {
    let ChatChoice {
        index,
        message,
        finish_reason,
        ..
    } = choice;

    let mut parts: Vec<GeminiPart> = Vec::new();

    if let Some(text) = message_text(&message) {
        if !text.is_empty() {
            parts.push(GeminiPart::plain_text(text));
        }
    }

    if let Some(tool_calls) = message.tool_calls {
        for tc in tool_calls {
            let args = match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                Ok(v) => v,
                Err(_) => serde_json::Value::String(tc.function.arguments),
            };
            parts.push(GeminiPart::FunctionCall {
                function_call: GeminiFunctionCall {
                    name: tc.function.name,
                    args,
                },
            });
        }
    }

    Ok(GeminiCandidate {
        index,
        content: Some(GeminiContent {
            role: Some("model".to_string()),
            parts,
        }),
        finish_reason: Some(finish_reason_to_gemini(finish_reason)),
        safety_ratings: Vec::new(),
        grounding_metadata: None,
        citation_metadata: None,
    })
}

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

fn finish_reason_to_gemini(reason: Option<FinishReason>) -> String {
    match reason {
        Some(FinishReason::Stop) => "STOP".to_string(),
        Some(FinishReason::Length) => "MAX_TOKENS".to_string(),
        Some(FinishReason::ContentFilter) => "SAFETY".to_string(),
        // Gemini 没有 TOOL_CALLS 专用原因，社区约定用 STOP
        Some(FinishReason::ToolCalls) | Some(FinishReason::FunctionCall) => "STOP".to_string(),
        None => "STOP".to_string(),
    }
}

fn usage_to_gemini(usage: &Usage) -> GeminiUsageMetadata {
    let cached = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens);
    GeminiUsageMetadata {
        prompt_token_count: usage.prompt_tokens,
        candidates_token_count: usage.completion_tokens,
        total_token_count: usage.total_tokens,
        cached_content_token_count: cached,
        thoughts_token_count: None,
    }
}

// ---------------------------------------------------------------------------
// from_canonical_stream_event
// ---------------------------------------------------------------------------

/// Gemini 流每块都是一个完整的 GeminiChatResponse（JSON 数组的一个元素）。
///
/// 当前支持的映射：
/// - `TextDelta { text }` → 一个 response，candidate 的 parts 里只有一条 text
/// - `End { finish_reason, usage }` → 最后一块带 finishReason + usageMetadata
/// - 其他事件（Start / ReasoningDelta / ToolCallDelta）目前产出空 Vec
fn from_canonical_stream_event_impl(
    event: ChatStreamEvent,
    state: &mut GeminiStreamState,
    _ctx: &IngressCtx,
) -> Vec<GeminiChatResponse> {
    let mut out = Vec::new();
    match event {
        ChatStreamEvent::Start { .. } => {
            // Gemini 流没有 "start" 事件概念；头块由第一段 delta 承担
        }
        ChatStreamEvent::TextDelta { text } if !text.is_empty() => {
            out.push(GeminiChatResponse {
                candidates: vec![GeminiCandidate {
                    index: state.emitted_candidates,
                    content: Some(GeminiContent {
                        role: Some("model".to_string()),
                        parts: vec![GeminiPart::plain_text(text)],
                    }),
                    finish_reason: None,
                    safety_ratings: Vec::new(),
                    grounding_metadata: None,
                    citation_metadata: None,
                }],
                prompt_feedback: None,
                usage_metadata: None,
                model_version: None,
            });
            state.emitted_candidates += 1;
        }
        ChatStreamEvent::TextDelta { .. } => {}
        ChatStreamEvent::ReasoningDelta { .. } => {
            // Gemini 原生流没有 reasoning content 的直接对等，当前忽略
        }
        ChatStreamEvent::ToolCallDelta(_) => {
            // 流式工具调用增量 pending（见文件头 Known limitations）
        }
        ChatStreamEvent::ThoughtSignature { .. } => {
            // Gemini 无对应的 thought signature 字段（仅 Claude extended thinking 有）。
        }
        ChatStreamEvent::UsageDelta(_) => {
            // Gemini wire 的 usage 由 End 事件派生的最终 usageMetadata 承载，
            // 中期 UsageDelta 只给 stream_driver 累计 final_usage 用。
        }
        ChatStreamEvent::Error(_) => {
            // Gemini 流协议没有 inline error event；stream_driver 会 break 并置 Failure，
            // HTTP 层的 trailer / 连接断开足以让客户端感知。
        }
        ChatStreamEvent::End(end) => {
            out.push(GeminiChatResponse {
                candidates: vec![GeminiCandidate {
                    index: state.emitted_candidates,
                    content: Some(GeminiContent {
                        role: Some("model".to_string()),
                        parts: Vec::new(),
                    }),
                    finish_reason: Some(finish_reason_to_gemini(end.finish_reason)),
                    safety_ratings: Vec::new(),
                    grounding_metadata: None,
                    citation_metadata: None,
                }],
                prompt_feedback: None,
                usage_metadata: end.usage.as_ref().map(usage_to_gemini),
                model_version: None,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::AdapterKind;

    fn ctx() -> IngressCtx {
        IngressCtx::new(AdapterKind::Gemini, "gemini-2.5-flash", "gemini-2.5-flash")
    }

    // ───── to_canonical ─────

    #[test]
    fn minimal_request() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}]
        }))
        .unwrap();
        let canonical = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(canonical.messages.len(), 1);
        assert_eq!(canonical.messages[0].role, Role::User);
        match canonical.messages[0].content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "hi"),
            _ => panic!(),
        }
    }

    #[test]
    fn system_instruction_becomes_system_message() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "systemInstruction": {"parts": [{"text": "you are helpful"}]}
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.messages.len(), 2);
        assert_eq!(c.messages[0].role, Role::System);
    }

    #[test]
    fn role_model_maps_to_assistant() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [
                {"role": "user", "parts": [{"text": "hi"}]},
                {"role": "model", "parts": [{"text": "hello"}]}
            ]
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.messages[1].role, Role::Assistant);
    }

    #[test]
    fn inline_data_becomes_data_uri() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [
                {"text": "describe"},
                {"inlineData": {"mimeType": "image/png", "data": "XYZ"}}
            ]}]
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        match c.messages[0].content.as_ref().unwrap() {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                if let ContentPart::ImageUrl { image_url } = &parts[1] {
                    assert!(image_url.url.starts_with("data:image/png;base64,"));
                } else {
                    panic!();
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn function_call_becomes_tool_calls() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [
                {"role": "model", "parts": [
                    {"text": "let me check"},
                    {"functionCall": {"name": "weather", "args": {"city": "NYC"}}}
                ]}
            ]
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        let msg = &c.messages[0];
        assert_eq!(msg.role, Role::Assistant);
        let tcs = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "weather");
        assert!(tcs[0].id.starts_with("call_"));
    }

    #[test]
    fn function_response_splits_into_tool_message() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [
                {"role": "model", "parts": [
                    {"functionCall": {"name": "weather", "args": {}}}
                ]},
                {"role": "user", "parts": [
                    {"functionResponse": {"name": "weather", "response": {"temp": "72F"}}},
                    {"text": "and then?"}
                ]}
            ]
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        // assistant / tool / user
        assert_eq!(c.messages.len(), 3);
        assert_eq!(c.messages[1].role, Role::Tool);
        assert_eq!(c.messages[1].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(c.messages[2].role, Role::User);
    }

    #[test]
    fn tools_functionDeclarations_flattened() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "tools": [{
                "functionDeclarations": [
                    {"name": "a", "parameters": {"type": "object"}},
                    {"name": "b", "parameters": {"type": "object"}}
                ]
            }]
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        let tools = c.tools.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].function.as_ref().unwrap().name, "a");
        assert_eq!(tools[1].function.as_ref().unwrap().name, "b");
    }

    #[test]
    fn tool_config_mode_maps() {
        let make = |cfg: serde_json::Value| {
            let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
                "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
                "toolConfig": cfg
            }))
            .unwrap();
            GeminiIngress::to_canonical(req, &ctx()).unwrap()
        };

        let c = make(serde_json::json!({"functionCallingConfig": {"mode": "AUTO"}}));
        match c.tool_choice.unwrap() {
            ToolChoice::Mode(s) => assert_eq!(s, "auto"),
            _ => panic!(),
        }

        let c = make(serde_json::json!({"functionCallingConfig": {"mode": "NONE"}}));
        match c.tool_choice.unwrap() {
            ToolChoice::Mode(s) => assert_eq!(s, "none"),
            _ => panic!(),
        }

        let c = make(serde_json::json!({"functionCallingConfig": {"mode": "ANY"}}));
        match c.tool_choice.unwrap() {
            ToolChoice::Mode(s) => assert_eq!(s, "required"),
            _ => panic!(),
        }

        let c = make(serde_json::json!({
            "functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": ["x"]}
        }));
        match c.tool_choice.unwrap() {
            ToolChoice::Named(v) => assert_eq!(v["function"]["name"], "x"),
            _ => panic!(),
        }
    }

    #[test]
    fn generation_config_maps_camel_to_canonical() {
        let req: GeminiGenerateContentRequest = serde_json::from_value(serde_json::json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "generationConfig": {
                "temperature": 0.7,
                "topP": 0.9,
                "topK": 40,
                "candidateCount": 2,
                "maxOutputTokens": 512,
                "stopSequences": ["a", "b", "c", "d", "e"],
                "thinkingConfig": {"thinkingBudget": 2048}
            }
        }))
        .unwrap();
        let c = GeminiIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(c.temperature, Some(0.7));
        assert_eq!(c.top_p, Some(0.9));
        assert_eq!(c.n, Some(2));
        assert_eq!(c.max_tokens, Some(512));
        assert_eq!(
            c.reasoning_effort,
            Some(summer_ai_core::ReasoningEffort::Medium)
        );
        // top_k 透传到 extra
        assert_eq!(c.extra["top_k"], 40);
        // stopSequences 裁剪到前 4
        match c.stop.unwrap() {
            serde_json::Value::Array(arr) => assert_eq!(arr.len(), 4),
            _ => panic!(),
        }
    }

    // ───── from_canonical ─────

    #[test]
    fn response_text_only() {
        let resp = ChatResponse {
            id: "chat-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "gemini-2.5-flash".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage::assistant("hello"),
                logprobs: None,
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: Usage {
                prompt_tokens: 3,
                completion_tokens: 5,
                total_tokens: 8,
                ..Default::default()
            },
            system_fingerprint: None,
            service_tier: None,
        };
        let gem = GeminiIngress::from_canonical(resp, &ctx()).unwrap();
        assert_eq!(gem.candidates.len(), 1);
        let candidate = &gem.candidates[0];
        assert_eq!(candidate.finish_reason.as_deref(), Some("STOP"));
        match &candidate.content.as_ref().unwrap().parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "hello"),
            _ => panic!(),
        }
        let u = gem.usage_metadata.unwrap();
        assert_eq!(u.prompt_token_count, 3);
        assert_eq!(u.total_token_count, 8);
    }

    #[test]
    fn response_tool_calls() {
        let resp = ChatResponse {
            id: "chat-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "gemini-2.5-flash".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: Role::Assistant,
                    content: None,
                    reasoning_content: None,
                    refusal: None,
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_1".to_string(),
                        kind: "function".to_string(),
                        function: ToolCallFunction {
                            name: "weather".to_string(),
                            arguments: r#"{"city":"NYC"}"#.to_string(),
                        },
                        thought_signatures: None,
                    }]),
                    tool_call_id: None,
                    audio: None,
                    options: None,
                },
                logprobs: None,
                finish_reason: Some(FinishReason::ToolCalls),
            }],
            usage: Usage::default(),
            system_fingerprint: None,
            service_tier: None,
        };
        let gem = GeminiIngress::from_canonical(resp, &ctx()).unwrap();
        let parts = &gem.candidates[0].content.as_ref().unwrap().parts;
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            GeminiPart::FunctionCall { function_call } => {
                assert_eq!(function_call.name, "weather");
                assert_eq!(function_call.args["city"], "NYC");
            }
            _ => panic!(),
        }
        // finish_reason tool_calls → STOP
        assert_eq!(gem.candidates[0].finish_reason.as_deref(), Some("STOP"));
    }

    #[test]
    fn finish_reason_mapping() {
        assert_eq!(finish_reason_to_gemini(Some(FinishReason::Stop)), "STOP");
        assert_eq!(
            finish_reason_to_gemini(Some(FinishReason::Length)),
            "MAX_TOKENS"
        );
        assert_eq!(
            finish_reason_to_gemini(Some(FinishReason::ContentFilter)),
            "SAFETY"
        );
        assert_eq!(
            finish_reason_to_gemini(Some(FinishReason::ToolCalls)),
            "STOP"
        );
        assert_eq!(finish_reason_to_gemini(None), "STOP");
    }

    // ───── from_canonical_stream_event ─────

    #[test]
    fn stream_text_delta_produces_one_chunk() {
        let mut state = StreamConvertState::for_format(IngressFormat::Gemini);
        let ctx = ctx();
        let out = GeminiIngress::from_canonical_stream_event(
            ChatStreamEvent::TextDelta {
                text: "hi".to_string(),
            },
            &mut state,
            &ctx,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        let parts = &out[0].candidates[0].content.as_ref().unwrap().parts;
        match &parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "hi"),
            _ => panic!(),
        }
    }

    #[test]
    fn stream_end_emits_finish_and_usage() {
        let mut state = StreamConvertState::for_format(IngressFormat::Gemini);
        let ctx = ctx();
        let out = GeminiIngress::from_canonical_stream_event(
            ChatStreamEvent::End(summer_ai_core::StreamEnd {
                finish_reason: Some(FinishReason::Stop),
                usage: Some(Usage {
                    prompt_tokens: 2,
                    completion_tokens: 3,
                    total_tokens: 5,
                    ..Default::default()
                }),
            }),
            &mut state,
            &ctx,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].candidates[0].finish_reason.as_deref(), Some("STOP"));
        let u = out[0].usage_metadata.as_ref().unwrap();
        assert_eq!(u.total_token_count, 5);
    }

    #[test]
    fn stream_empty_text_delta_produces_nothing() {
        let mut state = StreamConvertState::for_format(IngressFormat::Gemini);
        let ctx = ctx();
        let out = GeminiIngress::from_canonical_stream_event(
            ChatStreamEvent::TextDelta {
                text: String::new(),
            },
            &mut state,
            &ctx,
        )
        .unwrap();
        assert!(out.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
