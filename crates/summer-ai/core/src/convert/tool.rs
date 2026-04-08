use crate::types::common::Tool;

pub fn tool_names(tools: Option<&[Tool]>) -> Vec<&str> {
    tools
        .into_iter()
        .flatten()
        .map(|tool| tool.function.name.as_str())
        .collect()
}

pub fn parse_function_arguments(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments).unwrap_or_else(|error| {
        tracing::warn!(arguments, error = %error, "failed to parse tool call arguments as JSON, passing as raw string");
        serde_json::Value::String(arguments.into())
    })
}

pub fn serialize_arguments(arguments: serde_json::Value) -> String {
    match arguments {
        serde_json::Value::String(arguments) => arguments,
        other => serde_json::to_string(&other).unwrap_or_else(|_| "{}".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::FunctionDef;

    #[test]
    fn tool_names_returns_declared_function_names() {
        let tools = vec![
            Tool {
                r#type: "function".into(),
                function: FunctionDef {
                    name: "get_weather".into(),
                    description: None,
                    parameters: None,
                },
            },
            Tool {
                r#type: "function".into(),
                function: FunctionDef {
                    name: "get_news".into(),
                    description: None,
                    parameters: None,
                },
            },
        ];

        assert_eq!(tool_names(Some(&tools)), vec!["get_weather", "get_news"]);
        assert!(tool_names(None).is_empty());
    }

    #[test]
    fn parse_function_arguments_parses_json_and_falls_back_to_string() {
        assert_eq!(
            parse_function_arguments(r#"{"city":"Paris"}"#),
            serde_json::json!({"city": "Paris"})
        );
        assert_eq!(
            parse_function_arguments("not-json"),
            serde_json::json!("not-json")
        );
    }

    #[test]
    fn serialize_arguments_preserves_string_and_serializes_objects() {
        assert_eq!(serialize_arguments(serde_json::json!("raw")), "raw");
        assert_eq!(
            serialize_arguments(serde_json::json!({"city": "Paris"})),
            r#"{"city":"Paris"}"#
        );
    }
}
