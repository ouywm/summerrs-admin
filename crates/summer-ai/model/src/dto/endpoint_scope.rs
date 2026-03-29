use std::collections::HashSet;

use serde_json::Value;

pub const KNOWN_ENDPOINT_SCOPES: &[&str] = &[
    "chat",
    "completions",
    "responses",
    "embeddings",
    "images",
    "audio",
    "moderations",
    "rerank",
    "files",
    "batches",
    "assistants",
    "threads",
    "vector_stores",
    "fine_tuning",
    "uploads",
    "models",
];

pub fn default_endpoint_scope_array() -> Value {
    Value::Array(Vec::new())
}

pub fn normalize_endpoint_scope_list(
    value: &Value,
    field_name: &'static str,
) -> Result<Vec<String>, String> {
    let items = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items,
        _ => return Err(format!("{field_name} must be an array of strings")),
    };

    let mut scopes = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());

    for item in items {
        let Some(scope) = item.as_str() else {
            return Err(format!("{field_name} must be an array of strings"));
        };

        let scope = scope.trim().to_ascii_lowercase();
        if scope.is_empty() {
            return Err(format!("{field_name} contains an empty endpoint scope"));
        }
        if !KNOWN_ENDPOINT_SCOPES.contains(&scope.as_str()) {
            return Err(format!(
                "unsupported endpoint scope in {field_name}: {scope}"
            ));
        }
        if seen.insert(scope.clone()) {
            scopes.push(scope);
        }
    }

    Ok(scopes)
}

pub fn normalize_endpoint_scope_value(
    value: Value,
    field_name: &'static str,
) -> Result<Value, String> {
    Ok(Value::Array(
        normalize_endpoint_scope_list(&value, field_name)?
            .into_iter()
            .map(Value::String)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        default_endpoint_scope_array, normalize_endpoint_scope_list, normalize_endpoint_scope_value,
    };

    #[test]
    fn default_endpoint_scope_array_returns_empty_json_array() {
        assert_eq!(default_endpoint_scope_array(), serde_json::json!([]));
    }

    #[test]
    fn normalize_endpoint_scope_list_accepts_null_as_empty() {
        assert_eq!(
            normalize_endpoint_scope_list(&serde_json::Value::Null, "endpointScopes").unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn normalize_endpoint_scope_list_trims_lowercases_and_deduplicates() {
        assert_eq!(
            normalize_endpoint_scope_list(
                &serde_json::json!([" Chat ", "responses", "chat", "THREADS"]),
                "endpointScopes"
            )
            .unwrap(),
            vec![
                "chat".to_string(),
                "responses".to_string(),
                "threads".to_string()
            ]
        );
    }

    #[test]
    fn normalize_endpoint_scope_list_rejects_non_array_values() {
        let error = normalize_endpoint_scope_list(&serde_json::json!("chat"), "endpointScopes")
            .unwrap_err();
        assert_eq!(error, "endpointScopes must be an array of strings");
    }

    #[test]
    fn normalize_endpoint_scope_list_rejects_unknown_scope() {
        let error =
            normalize_endpoint_scope_list(&serde_json::json!(["chat", "foo"]), "endpointScopes")
                .unwrap_err();
        assert_eq!(error, "unsupported endpoint scope in endpointScopes: foo");
    }

    #[test]
    fn normalize_endpoint_scope_value_returns_normalized_json_array() {
        assert_eq!(
            normalize_endpoint_scope_value(
                serde_json::json!(["responses", " CHAT ", "responses"]),
                "supportedEndpoints"
            )
            .unwrap(),
            serde_json::json!(["responses", "chat"])
        );
    }
}
