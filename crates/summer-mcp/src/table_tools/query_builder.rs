use std::collections::BTreeMap;

use rmcp::ErrorData as McpError;
use sea_orm::{JsonValue, Value};
use serde::Deserialize;

use crate::table_tools::schema::{
    TableColumnSchema, TableSchema, bind_cast_value, ensure_valid_identifier, quote_identifier,
};

type JsonMap = BTreeMap<String, JsonValue>;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    ILike,
    In,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableFilter {
    /// 条件列名
    pub column: String,
    /// 条件操作符
    pub op: FilterOp,
    /// 单值条件参数
    pub value: Option<JsonValue>,
    /// 多值条件参数，仅用于 in
    pub values: Option<Vec<JsonValue>>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableSort {
    /// 排序列名
    pub column: String,
    /// 排序方向，默认 asc
    pub direction: Option<SortDirection>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum TableSortInput {
    /// 结构化排序参数，如 {"column":"id","direction":"desc"}
    Structured(TableSort),
    /// 简写排序参数，如 "id desc"
    Shorthand(String),
}

pub(crate) fn build_insert_assignments(
    schema: &TableSchema,
    values: &JsonMap,
) -> Result<(String, String, Vec<Value>), McpError> {
    if values.is_empty() {
        return Err(McpError::invalid_params(
            "insert values cannot be empty",
            None,
        ));
    }

    let mut columns = Vec::new();
    let mut expressions = Vec::new();
    let mut params = Vec::new();
    for (column_name, value) in values {
        ensure_valid_identifier(column_name, "column")?;
        let Some(column) = schema.column(column_name) else {
            return Err(McpError::invalid_params(
                format!(
                    "unknown column `{column_name}` for table `{}`",
                    schema.table
                ),
                None,
            ));
        };
        if !column.writable_on_create {
            return Err(McpError::invalid_params(
                format!(
                    "column `{column_name}` on table `{}` is not writable on create",
                    schema.table
                ),
                None,
            ));
        }

        columns.push(quote_identifier(&column.name));
        let placeholder = bind_cast_value(params.len() + 1, column, value, &mut params)?;
        expressions.push(placeholder);
    }

    Ok((columns.join(", "), expressions.join(", "), params))
}

pub(crate) fn build_update_assignments(
    schema: &TableSchema,
    values: &JsonMap,
) -> Result<(String, Vec<Value>), McpError> {
    if values.is_empty() {
        return Err(McpError::invalid_params(
            "update values cannot be empty",
            None,
        ));
    }

    let mut assignments = Vec::new();
    let mut params = Vec::new();
    for (column_name, value) in values {
        ensure_valid_identifier(column_name, "column")?;
        let Some(column) = schema.column(column_name) else {
            return Err(McpError::invalid_params(
                format!(
                    "unknown column `{column_name}` for table `{}`",
                    schema.table
                ),
                None,
            ));
        };
        if !column.writable_on_update {
            return Err(McpError::invalid_params(
                format!(
                    "column `{column_name}` on table `{}` is not writable on update",
                    schema.table
                ),
                None,
            ));
        }

        let placeholder = bind_cast_value(params.len() + 1, column, value, &mut params)?;
        assignments.push(format!(
            "{} = {placeholder}",
            quote_identifier(&column.name)
        ));
    }

    Ok((assignments.join(", "), params))
}

pub(crate) fn build_key_clause(
    schema: &TableSchema,
    key: &JsonMap,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    let primary_key_columns = schema.primary_key_columns();
    if primary_key_columns.is_empty() {
        return Err(McpError::invalid_params(
            format!("table `{}` does not have a primary key", schema.table),
            None,
        ));
    }

    if key.len() != primary_key_columns.len() {
        return Err(McpError::invalid_params(
            format!(
                "table `{}` requires key fields {:?}",
                schema.table, schema.primary_key
            ),
            None,
        ));
    }

    let mut clauses = Vec::with_capacity(primary_key_columns.len());
    for column in primary_key_columns {
        let Some(value) = key.get(&column.name) else {
            return Err(McpError::invalid_params(
                format!(
                    "missing primary key column `{}` for table `{}`",
                    column.name, schema.table
                ),
                None,
            ));
        };

        if value.is_null() {
            return Err(McpError::invalid_params(
                format!(
                    "primary key column `{}` for table `{}` cannot be null",
                    column.name, schema.table
                ),
                None,
            ));
        }

        let placeholder = bind_cast_value(params.len() + 1, column, value, params)?;
        clauses.push(format!(
            "{} = {placeholder}",
            quote_identifier(&column.name)
        ));
    }

    Ok(clauses.join(" AND "))
}

pub(crate) fn build_filters_clause(
    schema: &TableSchema,
    filters: Option<&[TableFilter]>,
) -> Result<(Option<String>, Vec<Value>), McpError> {
    let Some(filters) = filters else {
        return Ok((None, Vec::new()));
    };

    let mut clauses = Vec::with_capacity(filters.len());
    let mut params = Vec::new();
    for filter in filters {
        ensure_valid_identifier(&filter.column, "column")?;
        let Some(column) = schema.column(&filter.column) else {
            return Err(McpError::invalid_params(
                format!(
                    "unknown filter column `{}` for table `{}`",
                    filter.column, schema.table
                ),
                None,
            ));
        };
        if column.hidden_on_read {
            return Err(McpError::invalid_params(
                format!(
                    "column `{}` on table `{}` is hidden and cannot be filtered",
                    filter.column, schema.table
                ),
                None,
            ));
        }

        clauses.push(build_filter_clause(column, filter, &mut params)?);
    }

    if clauses.is_empty() {
        Ok((None, params))
    } else {
        Ok((Some(clauses.join(" AND ")), params))
    }
}

fn build_filter_clause(
    column: &TableColumnSchema,
    filter: &TableFilter,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    let quoted = quote_identifier(&column.name);
    match filter.op {
        FilterOp::Eq => {
            let value = require_single_filter_value(filter)?;
            if value.is_null() {
                return Ok(format!("{quoted} IS NULL"));
            }
            let placeholder = bind_cast_value(params.len() + 1, column, value, params)?;
            Ok(format!("{quoted} = {placeholder}"))
        }
        FilterOp::Ne => {
            let value = require_single_filter_value(filter)?;
            if value.is_null() {
                return Ok(format!("{quoted} IS NOT NULL"));
            }
            let placeholder = bind_cast_value(params.len() + 1, column, value, params)?;
            Ok(format!("{quoted} <> {placeholder}"))
        }
        FilterOp::Gt => compare_clause(column, &quoted, ">", filter, params),
        FilterOp::Gte => compare_clause(column, &quoted, ">=", filter, params),
        FilterOp::Lt => compare_clause(column, &quoted, "<", filter, params),
        FilterOp::Lte => compare_clause(column, &quoted, "<=", filter, params),
        FilterOp::Like => pattern_clause(&quoted, "LIKE", filter, params),
        FilterOp::ILike => pattern_clause(&quoted, "ILIKE", filter, params),
        FilterOp::In => {
            let values = filter.values.as_ref().ok_or_else(|| {
                McpError::invalid_params(
                    format!("filter `{}` requires `values`", column.name),
                    None,
                )
            })?;
            if values.is_empty() {
                return Err(McpError::invalid_params(
                    format!(
                        "filter `{}` requires at least one `values` item",
                        column.name
                    ),
                    None,
                ));
            }

            let mut expressions = Vec::with_capacity(values.len());
            for value in values {
                if value.is_null() {
                    return Err(McpError::invalid_params(
                        format!(
                            "filter `{}` does not support null inside `values`",
                            column.name
                        ),
                        None,
                    ));
                }
                expressions.push(bind_cast_value(params.len() + 1, column, value, params)?);
            }
            Ok(format!("{quoted} IN ({})", expressions.join(", ")))
        }
        FilterOp::IsNull => Ok(format!("{quoted} IS NULL")),
        FilterOp::IsNotNull => Ok(format!("{quoted} IS NOT NULL")),
    }
}

fn compare_clause(
    column: &TableColumnSchema,
    quoted: &str,
    operator: &str,
    filter: &TableFilter,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    let value = require_single_filter_value(filter)?;
    if value.is_null() {
        return Err(McpError::invalid_params(
            format!("operator `{operator}` does not support null"),
            None,
        ));
    }
    let placeholder = bind_cast_value(params.len() + 1, column, value, params)?;
    Ok(format!("{quoted} {operator} {placeholder}"))
}

fn pattern_clause(
    quoted: &str,
    operator: &str,
    filter: &TableFilter,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    let value = require_single_filter_value(filter)?;
    let Some(pattern) = value.as_str() else {
        return Err(McpError::invalid_params(
            format!("operator `{operator}` requires a string value"),
            None,
        ));
    };

    params.push(Value::from(pattern.to_string()));
    Ok(format!(
        "CAST({quoted} AS text) {operator} ${}",
        params.len()
    ))
}

fn require_single_filter_value(filter: &TableFilter) -> Result<&JsonValue, McpError> {
    filter
        .value
        .as_ref()
        .ok_or_else(|| McpError::invalid_params("filter requires `value`", None))
}

pub(crate) fn build_order_clause(
    schema: &TableSchema,
    order_by: Option<&[TableSortInput]>,
) -> Result<String, McpError> {
    let mut parts = Vec::new();
    if let Some(order_by) = order_by {
        for sort in order_by {
            let sort = parse_sort_input(sort)?;
            ensure_valid_identifier(&sort.column, "column")?;
            let Some(column) = schema.column(&sort.column) else {
                return Err(McpError::invalid_params(
                    format!(
                        "unknown order column `{}` for table `{}`",
                        sort.column, schema.table
                    ),
                    None,
                ));
            };
            if column.hidden_on_read {
                return Err(McpError::invalid_params(
                    format!(
                        "column `{}` on table `{}` is hidden and cannot be used for sorting",
                        sort.column, schema.table
                    ),
                    None,
                ));
            }

            let direction = match sort.direction.unwrap_or(SortDirection::Asc) {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            parts.push(format!("{} {direction}", quote_identifier(&column.name)));
        }
    }

    if parts.is_empty() {
        let primary_key = schema.primary_key_columns();
        if primary_key.is_empty() {
            return Ok(String::new());
        }

        parts = primary_key
            .into_iter()
            .map(|column| format!("{} ASC", quote_identifier(&column.name)))
            .collect();
    }

    Ok(format!(" ORDER BY {}", parts.join(", ")))
}

fn parse_sort_input(sort: &TableSortInput) -> Result<TableSort, McpError> {
    match sort {
        TableSortInput::Structured(sort) => Ok(sort.clone()),
        TableSortInput::Shorthand(sort) => parse_sort_shorthand(sort),
    }
}

fn parse_sort_shorthand(input: &str) -> Result<TableSort, McpError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            "order_by string item cannot be empty",
            None,
        ));
    }

    let mut parts = trimmed.split_whitespace();
    let column = parts.next().unwrap().to_string();
    let direction = match parts.next() {
        None => None,
        Some(direction) if direction.eq_ignore_ascii_case("asc") => Some(SortDirection::Asc),
        Some(direction) if direction.eq_ignore_ascii_case("desc") => Some(SortDirection::Desc),
        Some(direction) => {
            return Err(McpError::invalid_params(
                format!("unsupported sort direction `{direction}`"),
                None,
            ));
        }
    };

    if parts.next().is_some() {
        return Err(McpError::invalid_params(
            format!("invalid order_by shorthand `{trimmed}`"),
            None,
        ));
    }

    Ok(TableSort { column, direction })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sort_shorthand_accepts_common_forms() {
        let sort = parse_sort_shorthand("id desc").unwrap();
        assert_eq!(sort.column, "id");
        assert_eq!(sort.direction, Some(SortDirection::Desc));

        let sort = parse_sort_shorthand("role_name").unwrap();
        assert_eq!(sort.column, "role_name");
        assert_eq!(sort.direction, None);
    }
}
