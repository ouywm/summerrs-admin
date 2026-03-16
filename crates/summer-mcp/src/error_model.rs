use std::borrow::Cow;

use rmcp::{
    ErrorData as McpError,
    model::{ErrorCode, JsonObject},
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct StructuredErrorData {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Value>,
}

#[derive(Debug, Clone, Copy)]
struct ErrorClassification {
    machine_code: &'static str,
    summary: &'static str,
    hint: Option<&'static str>,
}

pub(crate) fn invalid_params_error(
    machine_code: &'static str,
    message: impl Into<String>,
    hint: Option<&str>,
    detail: Option<String>,
    meta: Option<Value>,
) -> McpError {
    structured_error(
        ErrorCode::INVALID_PARAMS,
        Cow::Owned(message.into()),
        machine_code,
        hint,
        detail,
        meta,
        None,
    )
}

pub(crate) fn internal_error(
    machine_code: &'static str,
    message: impl Into<String>,
    hint: Option<&str>,
    detail: Option<String>,
    meta: Option<Value>,
) -> McpError {
    structured_error(
        ErrorCode::INTERNAL_ERROR,
        Cow::Owned(message.into()),
        machine_code,
        hint,
        detail,
        meta,
        None,
    )
}

pub(crate) fn resource_not_found_error(
    machine_code: &'static str,
    message: impl Into<String>,
    hint: Option<&str>,
    detail: Option<String>,
    meta: Option<Value>,
) -> McpError {
    structured_error(
        ErrorCode::RESOURCE_NOT_FOUND,
        Cow::Owned(message.into()),
        machine_code,
        hint,
        detail,
        meta,
        None,
    )
}

pub(crate) fn normalize_tool_error(tool: &'static str, error: McpError) -> McpError {
    normalize_error(error, Some(json!({ "tool": tool })))
}

pub(crate) fn normalize_resource_error(resource: &str, error: McpError) -> McpError {
    normalize_error(error, Some(json!({ "resource": resource })))
}

fn normalize_error(error: McpError, extra_meta: Option<Value>) -> McpError {
    if let Some(existing) = try_merge_existing_payload(&error, extra_meta.clone()) {
        return existing;
    }

    let detail = error.message.to_string();
    let classified = classify_error(error.code, &detail);
    structured_error(
        error.code,
        Cow::Borrowed(classified.summary),
        classified.machine_code,
        classified.hint,
        Some(detail),
        extra_meta,
        error.data,
    )
}

fn try_merge_existing_payload(error: &McpError, extra_meta: Option<Value>) -> Option<McpError> {
    let Value::Object(mut data) = error.data.clone()? else {
        return None;
    };
    if !data.contains_key("code") || !data.contains_key("message") {
        return None;
    }

    if let Some(extra_meta) = extra_meta {
        merge_meta(&mut data, extra_meta);
    }

    Some(McpError::new(
        error.code,
        error.message.clone(),
        Some(Value::Object(data)),
    ))
}

fn structured_error(
    error_code: ErrorCode,
    message: Cow<'static, str>,
    machine_code: &'static str,
    hint: Option<&str>,
    detail: Option<String>,
    meta: Option<Value>,
    upstream: Option<Value>,
) -> McpError {
    let payload = StructuredErrorData {
        code: machine_code.to_string(),
        message: message.to_string(),
        hint: hint.map(ToOwned::to_owned),
        detail,
        meta,
        upstream,
    };
    McpError::new(
        error_code,
        message,
        Some(serde_json::to_value(payload).expect("structured error payload should serialize")),
    )
}

fn merge_meta(data: &mut JsonObject, extra_meta: Value) {
    let Value::Object(extra_meta) = extra_meta else {
        data.insert("meta".to_string(), extra_meta);
        return;
    };

    match data.remove("meta") {
        Some(Value::Object(mut existing)) => {
            existing.extend(extra_meta);
            data.insert("meta".to_string(), Value::Object(existing));
        }
        Some(existing) => {
            data.insert(
                "meta".to_string(),
                json!({
                    "current": existing,
                    "context": extra_meta,
                }),
            );
        }
        None => {
            data.insert("meta".to_string(), Value::Object(extra_meta));
        }
    }
}

fn classify_error(code: ErrorCode, detail: &str) -> ErrorClassification {
    match code {
        ErrorCode::INVALID_PARAMS => classify_invalid_params(detail),
        ErrorCode::RESOURCE_NOT_FOUND => ErrorClassification {
            machine_code: "resource_not_found",
            summary: "Resource not found",
            hint: Some(
                "Call list_resources or list_resource_templates first to confirm the published resource paths.",
            ),
        },
        _ => classify_internal(detail),
    }
}

fn classify_invalid_params(detail: &str) -> ErrorClassification {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("unknown table `") {
        ErrorClassification {
            machine_code: "table_not_found",
            summary: "Table not found",
            hint: Some("Read schema://tables first to confirm the live table name."),
        }
    } else if lower.contains("unknown column `") {
        ErrorClassification {
            machine_code: "column_not_found",
            summary: "Column not found",
            hint: Some(
                "Read schema://table/{table} first to confirm readable and writable columns.",
            ),
        }
    } else if lower.contains("hidden") {
        ErrorClassification {
            machine_code: "hidden_column_access",
            summary: "Hidden column cannot be accessed",
            hint: Some(
                "Use the schema metadata to avoid hidden columns such as passwords or blocked audit fields.",
            ),
        }
    } else if lower.contains("identifier")
        && (lower.contains("invalid") || lower.contains("cannot be empty"))
    {
        ErrorClassification {
            machine_code: "invalid_identifier",
            summary: "Invalid identifier",
            hint: Some(
                "Use ASCII letters, digits, and underscores only, and start with a letter or underscore.",
            ),
        }
    } else if lower.contains("primary key") {
        ErrorClassification {
            machine_code: "invalid_primary_key",
            summary: "Invalid primary key input",
            hint: Some(
                "Read schema://table/{table} and pass every primary-key column with a non-null value.",
            ),
        }
    } else if lower.contains("filter") {
        ErrorClassification {
            machine_code: "invalid_filter",
            summary: "Invalid filter expression",
            hint: Some(
                "Use structured filters or supported shorthand such as `id = 1`, `status in [1,2]`, or grouped `or`/`and` filters.",
            ),
        }
    } else if lower.contains("order_by") || lower.contains("sort direction") {
        ErrorClassification {
            machine_code: "invalid_sort",
            summary: "Invalid sort expression",
            hint: Some(
                "Use order_by items like `id desc` or {\"column\":\"id\",\"direction\":\"desc\"}.",
            ),
        }
    } else if lower.contains("output_dir") {
        ErrorClassification {
            machine_code: "invalid_output_dir",
            summary: "Invalid output directory",
            hint: Some(
                "Pass a non-empty output_dir. Use an absolute temp path when you want a safe preview.",
            ),
        }
    } else {
        ErrorClassification {
            machine_code: "invalid_params",
            summary: "Invalid parameters",
            hint: None,
        }
    }
}

fn classify_internal(detail: &str) -> ErrorClassification {
    let lower = detail.to_ascii_lowercase();
    if looks_like_sql_param_type_mismatch(&lower) {
        ErrorClassification {
            machine_code: "sql_param_type_mismatch",
            summary: "SQL parameter type mismatch",
            hint: Some(
                "Pass numbers and booleans as native JSON values, or use typed params such as {\"kind\":\"bigint\",\"value\":\"13\"}.",
            ),
        }
    } else if lower.contains("failed to execute read-only sql query") {
        ErrorClassification {
            machine_code: "sql_query_failed",
            summary: "Read-only SQL query failed",
            hint: Some(
                "Check the SQL text, placeholder order, and whether the query can be expressed with table_query instead.",
            ),
        }
    } else if lower.contains("failed to execute sql statement") {
        ErrorClassification {
            machine_code: "sql_exec_failed",
            summary: "SQL statement failed",
            hint: Some(
                "Check the statement type, placeholder order, and whether the target table or column names are correct.",
            ),
        }
    } else if lower.contains("start read-only sql transaction")
        || lower.contains("start sql_exec transaction")
    {
        ErrorClassification {
            machine_code: "database_transaction_failed",
            summary: "Database transaction failed",
            hint: Some(
                "Check database connectivity and whether the PostgreSQL session is healthy.",
            ),
        }
    } else if lower.contains("failed to list public tables")
        || lower.contains("failed to describe table")
        || lower.contains("failed to query row from")
        || lower.contains("failed to count rows in")
        || lower.contains("failed to query rows from")
        || lower.contains("failed to insert row into")
        || lower.contains("failed to update row in")
        || lower.contains("failed to delete row from")
    {
        ErrorClassification {
            machine_code: "database_operation_failed",
            summary: "Database operation failed",
            hint: Some("Check the target schema, live table structure, and database connectivity."),
        }
    } else if lower.contains("failed to serialize") {
        ErrorClassification {
            machine_code: "serialization_failed",
            summary: "Serialization failed",
            hint: Some(
                "Check that the generated payload is valid JSON and that all values are serializable.",
            ),
        }
    } else if lower.contains("template")
        && (lower.contains("register") || lower.contains("load") || lower.contains("render"))
    {
        ErrorClassification {
            machine_code: "template_render_failed",
            summary: "Template rendering failed",
            hint: Some("Check the template name and required context fields before regenerating."),
        }
    } else if lower.contains("workspace root")
        || lower.contains("failed to read ")
        || lower.contains("failed to write ")
        || lower.contains("create export output directory")
    {
        ErrorClassification {
            machine_code: "filesystem_operation_failed",
            summary: "Filesystem operation failed",
            hint: Some("Check output_dir, workspace location, and filesystem permissions."),
        }
    } else {
        ErrorClassification {
            machine_code: "internal_error",
            summary: "Internal error",
            hint: None,
        }
    }
}

fn looks_like_sql_param_type_mismatch(detail: &str) -> bool {
    detail.contains("operator does not exist")
        || detail.contains("could not determine data type")
        || detail.contains("invalid input syntax")
        || detail.contains("cannot cast")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_tool_error_wraps_raw_invalid_params() {
        let error = McpError::invalid_params("unknown table `sys_demo`", None);
        let normalized = normalize_tool_error("schema_describe_table", error);

        assert_eq!(normalized.message, "Table not found");
        let data = normalized.data.unwrap();
        let payload = data.as_object().unwrap();
        assert_eq!(payload.get("code").unwrap(), "table_not_found");
        assert_eq!(
            payload
                .get("meta")
                .and_then(Value::as_object)
                .and_then(|meta| meta.get("tool"))
                .unwrap(),
            "schema_describe_table"
        );
    }

    #[test]
    fn normalize_tool_error_detects_sql_param_type_mismatch() {
        let error = McpError::internal_error(
            "failed to execute read-only SQL query: operator does not exist: bigint = text",
            None,
        );
        let normalized = normalize_tool_error("sql_query_readonly", error);

        assert_eq!(normalized.message, "SQL parameter type mismatch");
        let payload = normalized.data.unwrap();
        assert_eq!(payload["code"], "sql_param_type_mismatch");
        assert!(payload["hint"].as_str().unwrap().contains("typed params"));
    }

    #[test]
    fn normalize_preserves_existing_structured_payload_and_merges_meta() {
        let error = invalid_params_error(
            "validation_failed",
            "Validation failed",
            Some("Check required fields."),
            Some("field `name` is required".to_string()),
            Some(json!({ "source": "domain" })),
        );
        let normalized = normalize_tool_error("dict_tool", error);
        let payload = normalized.data.unwrap();
        assert_eq!(payload["code"], "validation_failed");
        assert_eq!(payload["meta"]["source"], "domain");
        assert_eq!(payload["meta"]["tool"], "dict_tool");
    }
}
