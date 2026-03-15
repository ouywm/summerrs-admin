use rmcp::ErrorData as McpError;
use sea_orm::Value;
use serde_json::Value as JsonValue;

const FORBIDDEN_READONLY_SQL_KEYWORDS: &[&str] = &[
    "alter",
    "begin",
    "call",
    "cluster",
    "comment",
    "commit",
    "copy",
    "create",
    "deallocate",
    "delete",
    "discard",
    "do",
    "drop",
    "execute",
    "grant",
    "insert",
    "listen",
    "lock",
    "merge",
    "notify",
    "prepare",
    "refresh",
    "reindex",
    "release",
    "reset",
    "revoke",
    "rollback",
    "savepoint",
    "set",
    "start",
    "truncate",
    "unlisten",
    "update",
    "vacuum",
];
const EXEC_SQL_KEYWORDS: &[&str] = &[
    "alter",
    "analyze",
    "call",
    "cluster",
    "comment",
    "copy",
    "create",
    "deallocate",
    "delete",
    "discard",
    "do",
    "drop",
    "execute",
    "grant",
    "insert",
    "listen",
    "lock",
    "merge",
    "notify",
    "prepare",
    "refresh",
    "reindex",
    "reset",
    "revoke",
    "set",
    "truncate",
    "unlisten",
    "update",
    "vacuum",
];

pub(crate) fn normalize_readonly_sql(sql: &str) -> Result<String, McpError> {
    let normalized = normalize_single_statement_sql(sql, "sql_query_readonly")?;
    let tokens = collect_sql_tokens(normalized)?;
    let Some(first_keyword) = tokens.first() else {
        return Err(McpError::invalid_params(
            "sql_query_readonly could not find a readable SQL statement",
            None,
        ));
    };

    if !matches!(first_keyword.as_str(), "select" | "with") {
        return Err(McpError::invalid_params(
            "sql_query_readonly only accepts SELECT or WITH ... SELECT statements",
            None,
        ));
    }

    if first_keyword == "with" && !tokens.iter().any(|token| token == "select") {
        return Err(McpError::invalid_params(
            "WITH query must eventually produce a SELECT result",
            None,
        ));
    }

    if let Some(keyword) = tokens
        .iter()
        .find(|token| FORBIDDEN_READONLY_SQL_KEYWORDS.contains(&token.as_str()))
    {
        return Err(McpError::invalid_params(
            format!(
                "sql_query_readonly rejected the statement because it contains forbidden keyword `{keyword}`"
            ),
            None,
        ));
    }

    Ok(normalized.to_string())
}

pub(crate) fn normalize_exec_sql(sql: &str) -> Result<String, McpError> {
    let normalized = normalize_single_statement_sql(sql, "sql_exec")?;
    let tokens = collect_sql_tokens(normalized)?;
    let Some(first_keyword) = tokens.first() else {
        return Err(McpError::invalid_params(
            "sql_exec could not find an executable SQL statement",
            None,
        ));
    };

    if first_keyword == "select" {
        return Err(McpError::invalid_params(
            "sql_exec does not accept SELECT statements; use sql_query_readonly instead",
            None,
        ));
    }

    if first_keyword == "with"
        && !tokens
            .iter()
            .any(|token| EXEC_SQL_KEYWORDS.contains(&token.as_str()))
    {
        return Err(McpError::invalid_params(
            "sql_exec detected a read-only WITH query; use sql_query_readonly instead",
            None,
        ));
    }

    Ok(normalized.to_string())
}

fn normalize_single_statement_sql<'a>(sql: &'a str, tool_name: &str) -> Result<&'a str, McpError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            format!("{tool_name} requires a non-empty `sql` value"),
            None,
        ));
    }

    Ok(trimmed
        .strip_suffix(';')
        .map(str::trim_end)
        .unwrap_or(trimmed))
}

/// Scan SQL source into a list of lowercased keyword tokens, skipping string literals,
/// quoted identifiers, comments, and dollar-quoted bodies.
///
/// The scanner recognises:
/// - `'...'` single-quoted strings (with `''` escape)
/// - `"..."` double-quoted identifiers (with `""` escape)
/// - `--` line comments
/// - `/* ... */` nested block comments
/// - `$tag$...$tag$` PostgreSQL dollar-quoted strings (tag may be empty, e.g. `$$...$$`)
/// - `E'...'` escape-string constants (treated as single-quoted after consuming the `E` prefix)
/// - `U&'...'` / `U&"..."` Unicode escape strings/identifiers
///
/// A semicolon outside any quoted context produces an error (we only allow single statements).
fn collect_sql_tokens(sql: &str) -> Result<Vec<String>, McpError> {
    #[derive(Debug)]
    enum ScanState {
        Normal,
        SingleQuoted,
        DoubleQuoted,
        LineComment,
        BlockComment(usize),
        /// PostgreSQL dollar-quoted string; the payload is the full opening tag
        /// including both `$` delimiters (e.g. `$$` or `$fn$`).
        DollarQuoted(Vec<u8>),
    }

    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut state = ScanState::Normal;
    let mut index = 0;

    while index < bytes.len() {
        match &mut state {
            ScanState::Normal => {
                if matches_bytes(bytes, index, b"--") {
                    state = ScanState::LineComment;
                    index += 2;
                    continue;
                }
                if matches_bytes(bytes, index, b"/*") {
                    state = ScanState::BlockComment(1);
                    index += 2;
                    continue;
                }
                // PostgreSQL E'...' escape-string constant — consume `E` and enter single-quoted
                if (bytes[index] == b'E' || bytes[index] == b'e')
                    && bytes.get(index + 1) == Some(&b'\'')
                {
                    state = ScanState::SingleQuoted;
                    index += 2;
                    continue;
                }
                // PostgreSQL U&'...' or U&"..." Unicode escape string/identifier
                if (bytes[index] == b'U' || bytes[index] == b'u')
                    && bytes.get(index + 1) == Some(&b'&')
                {
                    if bytes.get(index + 2) == Some(&b'\'') {
                        state = ScanState::SingleQuoted;
                        index += 3;
                        continue;
                    }
                    if bytes.get(index + 2) == Some(&b'"') {
                        state = ScanState::DoubleQuoted;
                        index += 3;
                        continue;
                    }
                }
                if bytes[index] == b'\'' {
                    state = ScanState::SingleQuoted;
                    index += 1;
                    continue;
                }
                if bytes[index] == b'"' {
                    state = ScanState::DoubleQuoted;
                    index += 1;
                    continue;
                }
                if let Some(tag) = parse_dollar_quote_tag(bytes, index) {
                    state = ScanState::DollarQuoted(tag.clone());
                    index += tag.len();
                    continue;
                }
                if bytes[index] == b';' {
                    return Err(McpError::invalid_params(
                        "SQL tools only accept a single SQL statement",
                        None,
                    ));
                }
                if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
                    let start = index;
                    index += 1;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                    {
                        index += 1;
                    }
                    tokens.push(sql[start..index].to_ascii_lowercase());
                    continue;
                }

                index += 1;
            }
            ScanState::SingleQuoted => {
                if bytes[index] == b'\'' {
                    if index + 1 < bytes.len() && bytes[index + 1] == b'\'' {
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[index] == b'"' {
                    if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::LineComment => {
                if bytes[index] == b'\n' {
                    state = ScanState::Normal;
                }
                index += 1;
            }
            ScanState::BlockComment(depth) => {
                if matches_bytes(bytes, index, b"/*") {
                    *depth += 1;
                    index += 2;
                } else if matches_bytes(bytes, index, b"*/") {
                    *depth -= 1;
                    index += 2;
                    if *depth == 0 {
                        state = ScanState::Normal;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::DollarQuoted(tag) => {
                if bytes[index..].starts_with(tag.as_slice()) {
                    index += tag.len();
                    state = ScanState::Normal;
                } else {
                    index += 1;
                }
            }
        }
    }

    match state {
        ScanState::Normal | ScanState::LineComment => Ok(tokens),
        ScanState::SingleQuoted => Err(McpError::invalid_params(
            "SQL statement contains an unterminated single-quoted string",
            None,
        )),
        ScanState::DoubleQuoted => Err(McpError::invalid_params(
            "SQL statement contains an unterminated quoted identifier",
            None,
        )),
        ScanState::BlockComment(_) => Err(McpError::invalid_params(
            "SQL statement contains an unterminated block comment",
            None,
        )),
        ScanState::DollarQuoted(_) => Err(McpError::invalid_params(
            "SQL statement contains an unterminated dollar-quoted string",
            None,
        )),
    }
}

/// Parse a PostgreSQL dollar-quote opening tag starting at `start`.
///
/// A dollar-quote tag has the form `$identifier$` where `identifier` is optional
/// (allowing the minimal `$$`). Returns the full tag bytes (including both `$`)
/// so the scanner can later search for the matching closing tag.
fn parse_dollar_quote_tag(bytes: &[u8], start: usize) -> Option<Vec<u8>> {
    if bytes.get(start) != Some(&b'$') {
        return None;
    }

    let mut end = start + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }

    if bytes.get(end) != Some(&b'$') {
        return None;
    }

    Some(bytes[start..=end].to_vec())
}

fn matches_bytes(haystack: &[u8], start: usize, needle: &[u8]) -> bool {
    haystack[start..].starts_with(needle)
}

pub(crate) fn convert_sql_params(params: &[JsonValue]) -> Result<Vec<Value>, McpError> {
    params.iter().map(json_param_to_value).collect()
}

fn json_param_to_value(value: &JsonValue) -> Result<Value, McpError> {
    Ok(match value {
        JsonValue::Null => Value::Json(None),
        JsonValue::Bool(value) => Value::from(*value),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::from(value)
            } else if let Some(value) = value.as_u64() {
                Value::from(value)
            } else if let Some(value) = value.as_f64() {
                Value::from(value)
            } else {
                return Err(McpError::invalid_params(
                    format!("unsupported numeric SQL parameter `{value}`"),
                    None,
                ));
            }
        }
        JsonValue::String(value) => Value::from(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => Value::from(value.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_sql_validator_accepts_safe_queries() {
        assert_eq!(
            normalize_readonly_sql(
                r#"
                with roles as (
                    select id, role_name from sys_role where role_name <> 'update'
                )
                select * from roles
                "#
            )
            .unwrap(),
            "with roles as (\n                    select id, role_name from sys_role where role_name <> 'update'\n                )\n                select * from roles"
        );
    }

    #[test]
    fn readonly_sql_validator_rejects_write_cte() {
        let error = normalize_readonly_sql(
            "with changed as (update sys_role set enabled = false returning id) select * from changed",
        )
        .unwrap_err();
        assert!(error.message.contains("forbidden keyword `update`"));
    }

    #[test]
    fn exec_sql_validator_rejects_select() {
        let error = normalize_exec_sql("select * from sys_role").unwrap_err();
        assert!(error.message.contains("use sql_query_readonly instead"));
    }

    #[test]
    fn exec_sql_validator_accepts_update_cte() {
        assert_eq!(
            normalize_exec_sql(
                "with changed as (update sys_role set enabled = false where id = 1 returning id) select count(*) from changed",
            )
            .unwrap(),
            "with changed as (update sys_role set enabled = false where id = 1 returning id) select count(*) from changed"
        );
    }

    #[test]
    fn scanner_handles_escape_string_constants() {
        // E'...' should not extract keywords from inside the string
        let tokens = collect_sql_tokens("SELECT * FROM t WHERE name = E'select'").unwrap();
        assert_eq!(tokens, vec!["select", "from", "t", "where", "name"]);
    }

    #[test]
    fn scanner_handles_unicode_escape_strings() {
        let tokens =
            collect_sql_tokens(r#"SELECT * FROM t WHERE name = U&'d\0061t\+000061'"#).unwrap();
        assert_eq!(tokens, vec!["select", "from", "t", "where", "name"]);
    }
}
