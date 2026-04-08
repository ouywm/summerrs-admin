use crate::types::common::Message;
use crate::types::common::Tool;
use crate::types::responses::ResponsesRequest;

pub fn message_text_content(message: &Message) -> Option<&str> {
    message.content.as_str()
}

pub fn join_message_text_by_role(messages: &[Message], role: &str) -> Option<String> {
    join_message_text_by_roles(messages, &[role])
}

pub fn join_message_text_by_roles(messages: &[Message], roles: &[&str]) -> Option<String> {
    let text = messages
        .iter()
        .filter(|message| roles.contains(&message.role.as_str()))
        .filter_map(|message| crate::convert::extract_text_segments(&message.content))
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!text.is_empty()).then_some(text)
}

pub fn stop_sequences_from_value(stop: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(stop) = stop.as_str() {
        return Some(vec![stop.to_string()]);
    }

    stop.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect()
    })
}

pub fn stop_sequences_from_option(stop: Option<&serde_json::Value>) -> Option<Vec<String>> {
    stop.and_then(stop_sequences_from_value)
}

pub(crate) fn responses_request_to_chat_request(
    req: &ResponsesRequest,
) -> crate::types::chat::ChatCompletionRequest {
    let mut messages = Vec::new();
    if let Some(instructions) = req.instructions.as_ref()
        && !instructions.is_empty()
    {
        messages.push(Message {
            role: "system".into(),
            content: serde_json::Value::String(instructions.clone()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }
    messages.extend(responses_input_to_messages(&req.input));

    let mut extra = req.extra.clone();
    if let Some(reasoning) = req.reasoning.as_ref() {
        extra.insert("reasoning".into(), reasoning.clone());
    }
    if let Some(metadata) = req.metadata.as_ref() {
        extra.insert("metadata".into(), metadata.clone());
    }

    crate::types::chat::ChatCompletionRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        max_tokens: req.max_output_tokens,
        top_p: req.top_p,
        frequency_penalty: None,
        presence_penalty: None,
        stop: None,
        tools: req.tools.as_ref().and_then(|tools| {
            serde_json::from_value::<Vec<Tool>>(tools.clone())
                .map_err(|error| {
                    tracing::warn!(error = %error, "failed to parse tools from request, ignoring tools");
                    error
                })
                .ok()
        }),
        tool_choice: req.tool_choice.clone(),
        response_format: req
            .text
            .as_ref()
            .and_then(|text| text.get("format"))
            .cloned(),
        stream_options: None,
        extra,
    }
}

pub(crate) fn merge_extra_body_fields(
    body: &mut serde_json::Value,
    extra: &serde_json::Map<String, serde_json::Value>,
) {
    let Some(body_obj) = body.as_object_mut() else {
        return;
    };

    for (key, value) in extra {
        body_obj.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

fn responses_input_to_messages(input: &serde_json::Value) -> Vec<Message> {
    match input {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => {
            vec![user_message(serde_json::Value::String(text.clone()))]
        }
        serde_json::Value::Array(items) => {
            let parsed: Option<Vec<Message>> =
                items.iter().map(response_input_item_to_message).collect();
            parsed.unwrap_or_else(|| vec![user_message(input.clone())])
        }
        _ => response_input_item_to_message(input)
            .map(|message| vec![message])
            .unwrap_or_else(|| vec![user_message(input.clone())]),
    }
}

fn response_input_item_to_message(value: &serde_json::Value) -> Option<Message> {
    if value.get("role").is_some() && value.get("content").is_some() {
        return serde_json::from_value::<Message>(value.clone()).ok();
    }

    let role = value.get("role").and_then(serde_json::Value::as_str)?;
    let content = value.get("content")?.clone();
    Some(Message {
        role: role.to_string(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
    })
}

fn user_message(content: serde_json::Value) -> Message {
    Message {
        role: "user".into(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::responses::ResponsesRequest;

    #[test]
    fn join_message_text_by_role_collects_matching_messages() {
        let messages = vec![
            Message {
                role: "system".into(),
                content: serde_json::json!("rule 1"),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".into(),
                content: serde_json::json!("hello"),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "system".into(),
                content: serde_json::json!([
                    {"type": "text", "text": "rule 2"},
                    {"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}}
                ]),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        assert_eq!(
            join_message_text_by_role(&messages, "system"),
            Some("rule 1\n\nrule 2".into())
        );
        assert_eq!(
            join_message_text_by_role(&messages, "user"),
            Some("hello".into())
        );
        assert_eq!(join_message_text_by_role(&messages, "assistant"), None);
    }

    #[test]
    fn join_message_text_by_roles_preserves_order_across_multiple_roles() {
        let messages = vec![
            Message {
                role: "system".into(),
                content: serde_json::json!("rule 1"),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "developer".into(),
                content: serde_json::json!("rule 2"),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".into(),
                content: serde_json::json!("hello"),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        assert_eq!(
            join_message_text_by_roles(&messages, &["system", "developer"]),
            Some("rule 1\n\nrule 2".into())
        );
    }

    #[test]
    fn responses_request_to_chat_request_preserves_instructions_tools_and_extra() {
        let request: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4.1",
            "instructions": "be concise",
            "input": [{"role": "user", "content": "hello"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {"type": "object"}
                }
            }],
            "tool_choice": "auto",
            "temperature": 0.2,
            "max_output_tokens": 64,
            "metadata": {"trace_id": "trace-1"},
            "reasoning": {"effort": "medium"},
            "custom_flag": true
        }))
        .unwrap();

        let chat_request = responses_request_to_chat_request(&request);
        assert_eq!(chat_request.model, "gpt-4.1");
        assert_eq!(chat_request.messages.len(), 2);
        assert_eq!(chat_request.messages[0].role, "system");
        assert_eq!(
            chat_request.messages[0].content,
            serde_json::json!("be concise")
        );
        assert_eq!(chat_request.messages[1].role, "user");
        assert_eq!(
            chat_request.tools.as_ref().unwrap()[0].function.name,
            "get_weather"
        );
        assert_eq!(chat_request.tool_choice, Some(serde_json::json!("auto")));
        assert_eq!(chat_request.temperature, Some(0.2));
        assert_eq!(chat_request.max_tokens, Some(64));
        assert_eq!(
            chat_request.extra.get("custom_flag"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            chat_request.extra.get("metadata"),
            Some(&serde_json::json!({"trace_id": "trace-1"}))
        );
        assert_eq!(
            chat_request.extra.get("reasoning"),
            Some(&serde_json::json!({"effort": "medium"}))
        );
    }

    #[test]
    fn merge_extra_body_fields_only_fills_missing_keys() {
        let mut body = serde_json::json!({
            "model": "gpt-4.1",
            "temperature": 0.1
        });
        let extra = serde_json::json!({
            "temperature": 0.8,
            "metadata": {"trace_id": "trace-1"}
        })
        .as_object()
        .unwrap()
        .clone();

        merge_extra_body_fields(&mut body, &extra);

        assert_eq!(body["temperature"], serde_json::json!(0.1));
        assert_eq!(body["metadata"], serde_json::json!({"trace_id": "trace-1"}));
    }

    #[test]
    fn stop_sequences_helpers_support_string_and_array() {
        assert_eq!(
            stop_sequences_from_value(&serde_json::json!("END")),
            Some(vec!["END".into()])
        );
        assert_eq!(
            stop_sequences_from_option(Some(&serde_json::json!(["END", "HALT"]))),
            Some(vec!["END".into(), "HALT".into()])
        );
        assert_eq!(stop_sequences_from_option(None), None);
    }
}
