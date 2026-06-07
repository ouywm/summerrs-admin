use rmcp::ErrorData as McpError;
use summer_common::error::ApiErrors;
use summer_system_model::dto::sys_dict::{DictDataQueryDto, DictTypeQueryDto};

use crate::{
    error_model::{internal_error, invalid_params_error},
    tools::support::error_chain_message,
};

pub(crate) fn api_error_to_mcp(error: ApiErrors) -> McpError {
    match error {
        ApiErrors::Internal(error) => internal_error(
            "business_operation_failed",
            "Business operation failed",
            None,
            Some(error_chain_message(error.as_ref())),
            None,
        ),
        ApiErrors::ServiceUnavailable(message) => internal_error(
            "service_unavailable",
            "Service unavailable",
            Some("Check dependent services and retry once the admin application is healthy."),
            Some(message),
            None,
        ),
        ApiErrors::BadRequest(message) => {
            invalid_params_error("bad_request", "Bad request", None, Some(message), None)
        }
        ApiErrors::Unauthorized(message) => invalid_params_error(
            "unauthorized",
            "Unauthorized",
            Some("Check the caller context or authentication setup before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::Forbidden(message) => invalid_params_error(
            "forbidden",
            "Forbidden",
            Some("Check permissions or route guards before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::NotFound(message) => invalid_params_error(
            "entity_not_found",
            "Entity not found",
            Some("Query the current business data first before updating or deleting it."),
            Some(message),
            None,
        ),
        ApiErrors::Conflict(message) => invalid_params_error(
            "conflict",
            "Conflict",
            Some(
                "Inspect current business data first; the write may violate a uniqueness or state rule.",
            ),
            Some(message),
            None,
        ),
        ApiErrors::IncompleteUpload(message) => invalid_params_error(
            "incomplete_upload",
            "Incomplete upload",
            None,
            Some(message),
            None,
        ),
        ApiErrors::ValidationFailed(message) => invalid_params_error(
            "validation_failed",
            "Validation failed",
            Some("Check required fields, enum values, and field formats before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::PayloadTooLarge(message) => invalid_params_error(
            "payload_too_large",
            "Payload too large",
            Some("Reduce payload size or adjust the server request body limit before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::TooManyRequests(message) => invalid_params_error(
            "rate_limited",
            "Too many requests",
            Some("Reduce request frequency and retry later."),
            Some(message),
            None,
        ),
    }
}

pub(crate) fn operator_name(operator: Option<String>) -> String {
    operator
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "mcp".to_string())
}

pub(crate) fn empty_dict_type_query() -> DictTypeQueryDto {
    DictTypeQueryDto {
        dict_name: None,
        dict_type: None,
        status: None,
    }
}

pub(crate) fn empty_dict_data_query() -> DictDataQueryDto {
    DictDataQueryDto {
        dict_type: None,
        dict_label: None,
        status: None,
    }
}

pub(crate) fn sql_tool_db_error(action: impl Into<String>, error: sea_orm::DbErr) -> McpError {
    let action = action.into();
    let detail = error_chain_message(&error);
    if looks_like_sql_param_type_mismatch(&detail) {
        return internal_error(
            "sql_param_type_mismatch",
            "SQL parameter type mismatch",
            Some(
                "Pass numbers and booleans as native JSON values, or use typed params such as {\"kind\":\"bigint\",\"value\":\"13\"}.",
            ),
            Some(detail),
            Some(serde_json::json!({ "action": action })),
        );
    }
    let machine_code = if action.contains("read-only SQL query") {
        "sql_query_failed"
    } else if action.contains("SQL statement") {
        "sql_exec_failed"
    } else {
        "database_transaction_failed"
    };
    internal_error(
        machine_code,
        "SQL operation failed",
        Some("Check the SQL text, placeholder order, and database connectivity."),
        Some(detail),
        Some(serde_json::json!({ "action": action })),
    )
}

fn looks_like_sql_param_type_mismatch(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("operator does not exist")
        || lower.contains("could not determine data type")
        || lower.contains("invalid input syntax")
        || lower.contains("cannot cast")
}
