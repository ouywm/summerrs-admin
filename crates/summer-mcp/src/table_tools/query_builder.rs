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
    NotIn,
    Between,
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
    /// 多值条件参数，用于 in / not_in / between
    pub values: Option<Vec<JsonValue>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableFilterGroup {
    /// 分组内全部条件都要满足
    pub and: Option<Vec<TableFilterInput>>,
    /// 分组内任一条件满足即可
    pub or: Option<Vec<TableFilterInput>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum TableFilterInput {
    /// 结构化过滤参数，如 {"column":"id","op":"eq","value":1}
    Structured(TableFilter),
    /// 结构化分组参数，如 {"or":[{"column":"status","op":"eq","value":1},{"column":"status","op":"eq","value":2}]}
    Group(TableFilterGroup),
    /// 简写过滤参数，如 "id = 1"、"name ilike admin"、"status in [1,2]"、"create_time between [\"2026-01-01\",\"2026-01-31\"]"
    Shorthand(String),
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
    filters: Option<&[TableFilterInput]>,
) -> Result<(Option<String>, Vec<Value>), McpError> {
    let Some(filters) = filters else {
        return Ok((None, Vec::new()));
    };

    let mut clauses = Vec::with_capacity(filters.len());
    let mut params = Vec::new();
    for filter_input in filters {
        clauses.push(build_filter_input_clause(
            schema,
            filter_input,
            &mut params,
        )?);
    }

    if clauses.is_empty() {
        Ok((None, params))
    } else {
        Ok((Some(clauses.join(" AND ")), params))
    }
}

fn build_filter_input_clause(
    schema: &TableSchema,
    filter_input: &TableFilterInput,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    match filter_input {
        TableFilterInput::Structured(filter) => build_single_filter_clause(schema, filter, params),
        TableFilterInput::Group(group) => build_filter_group_clause(schema, group, params),
        TableFilterInput::Shorthand(filter) => {
            let filter = parse_filter_shorthand(filter)?;
            build_single_filter_clause(schema, &filter, params)
        }
    }
}

fn build_filter_group_clause(
    schema: &TableSchema,
    group: &TableFilterGroup,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    match (group.and.as_ref(), group.or.as_ref()) {
        (Some(_), Some(_)) => Err(McpError::invalid_params(
            "filter group cannot contain both `and` and `or`",
            None,
        )),
        (None, None) => Err(McpError::invalid_params(
            "filter group requires either `and` or `or`",
            None,
        )),
        (Some(items), None) => build_filter_group_items(schema, items, "AND", params),
        (None, Some(items)) => build_filter_group_items(schema, items, "OR", params),
    }
}

fn build_filter_group_items(
    schema: &TableSchema,
    items: &[TableFilterInput],
    joiner: &str,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    if items.is_empty() {
        return Err(McpError::invalid_params(
            format!(
                "filter group `{}` requires at least one item",
                joiner.to_ascii_lowercase()
            ),
            None,
        ));
    }

    let mut clauses = Vec::with_capacity(items.len());
    for item in items {
        clauses.push(build_filter_input_clause(schema, item, params)?);
    }

    if clauses.len() == 1 {
        Ok(clauses.into_iter().next().unwrap())
    } else {
        Ok(format!("({})", clauses.join(&format!(" {joiner} "))))
    }
}

fn build_single_filter_clause(
    schema: &TableSchema,
    filter: &TableFilter,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
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

    build_filter_clause(column, filter, params)
}

fn parse_filter_shorthand(input: &str) -> Result<TableFilter, McpError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            "filter shorthand cannot be empty",
            None,
        ));
    }

    let mut parts = trimmed.splitn(3, char::is_whitespace);
    let column = parts
        .next()
        .ok_or_else(|| McpError::invalid_params("filter shorthand requires a column", None))?;
    let operator = parts
        .next()
        .ok_or_else(|| McpError::invalid_params("filter shorthand requires an operator", None))?;
    let remainder = parts.next().map(str::trim).unwrap_or_default();

    let (op, value, values) = match operator.to_ascii_lowercase().as_str() {
        "=" | "eq" => (
            FilterOp::Eq,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "!=" | "<>" | "ne" => (
            FilterOp::Ne,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        ">" | "gt" => (
            FilterOp::Gt,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        ">=" | "gte" => (
            FilterOp::Gte,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "<" | "lt" => (
            FilterOp::Lt,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "<=" | "lte" => (
            FilterOp::Lte,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "like" => (
            FilterOp::Like,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "ilike" => (
            FilterOp::ILike,
            Some(parse_filter_value_literal(remainder)?),
            None,
        ),
        "in" => (
            FilterOp::In,
            None,
            Some(parse_filter_values_literal(remainder)?),
        ),
        "not_in" => (
            FilterOp::NotIn,
            None,
            Some(parse_filter_values_literal(remainder)?),
        ),
        "between" => (
            FilterOp::Between,
            None,
            Some(parse_filter_values_literal(remainder)?),
        ),
        "is_null" => (FilterOp::IsNull, None, None),
        "is_not_null" => (FilterOp::IsNotNull, None, None),
        "is" if remainder.eq_ignore_ascii_case("null") => (FilterOp::IsNull, None, None),
        "is" if remainder.eq_ignore_ascii_case("not null") => (FilterOp::IsNotNull, None, None),
        other => {
            return Err(McpError::invalid_params(
                format!("unsupported filter operator shorthand `{other}`"),
                None,
            ));
        }
    };

    Ok(TableFilter {
        column: column.to_string(),
        op,
        value,
        values,
    })
}

fn parse_filter_value_literal(input: &str) -> Result<JsonValue, McpError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            "filter shorthand requires a value",
            None,
        ));
    }

    Ok(parse_jsonish_literal(trimmed))
}

fn parse_filter_values_literal(input: &str) -> Result<Vec<JsonValue>, McpError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            "filter shorthand `in` requires a JSON array value",
            None,
        ));
    }

    let parsed = serde_json::from_str::<JsonValue>(trimmed).map_err(|_| {
        McpError::invalid_params(
            "filter shorthand `in` requires a JSON array value like `[1,2,3]`",
            None,
        )
    })?;
    let Some(values) = parsed.as_array() else {
        return Err(McpError::invalid_params(
            "filter shorthand `in` requires a JSON array value like `[1,2,3]`",
            None,
        ));
    };
    if values.is_empty() {
        return Err(McpError::invalid_params(
            "filter shorthand `in` requires at least one array item",
            None,
        ));
    }
    Ok(values.clone())
}

fn parse_jsonish_literal(input: &str) -> JsonValue {
    serde_json::from_str::<JsonValue>(input)
        .unwrap_or_else(|_| JsonValue::String(input.to_string()))
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
            let expressions = build_multi_value_expressions(column, filter, params)?;
            Ok(format!("{quoted} IN ({})", expressions.join(", ")))
        }
        FilterOp::NotIn => {
            let expressions = build_multi_value_expressions(column, filter, params)?;
            Ok(format!("{quoted} NOT IN ({})", expressions.join(", ")))
        }
        FilterOp::Between => {
            let values = require_filter_values(filter, &column.name)?;
            if values.len() != 2 {
                return Err(McpError::invalid_params(
                    format!(
                        "filter `{}` with `between` requires exactly two values",
                        column.name
                    ),
                    None,
                ));
            }
            if values.iter().any(JsonValue::is_null) {
                return Err(McpError::invalid_params(
                    format!(
                        "filter `{}` with `between` does not support null values",
                        column.name
                    ),
                    None,
                ));
            }
            let start = bind_cast_value(params.len() + 1, column, &values[0], params)?;
            let end = bind_cast_value(params.len() + 1, column, &values[1], params)?;
            Ok(format!("{quoted} BETWEEN {start} AND {end}"))
        }
        FilterOp::IsNull => Ok(format!("{quoted} IS NULL")),
        FilterOp::IsNotNull => Ok(format!("{quoted} IS NOT NULL")),
    }
}

fn build_multi_value_expressions(
    column: &TableColumnSchema,
    filter: &TableFilter,
    params: &mut Vec<Value>,
) -> Result<Vec<String>, McpError> {
    let values = require_filter_values(filter, &column.name)?;
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
    Ok(expressions)
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

fn require_filter_values<'a>(
    filter: &'a TableFilter,
    column_name: &str,
) -> Result<&'a [JsonValue], McpError> {
    let values = filter.values.as_ref().ok_or_else(|| {
        McpError::invalid_params(format!("filter `{column_name}` requires `values`"), None)
    })?;
    if values.is_empty() {
        return Err(McpError::invalid_params(
            format!("filter `{column_name}` requires at least one `values` item"),
            None,
        ));
    }
    Ok(values)
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
    use serde_json::json;

    #[test]
    fn parse_sort_shorthand_accepts_common_forms() {
        let sort = parse_sort_shorthand("id desc").unwrap();
        assert_eq!(sort.column, "id");
        assert_eq!(sort.direction, Some(SortDirection::Desc));

        let sort = parse_sort_shorthand("role_name").unwrap();
        assert_eq!(sort.column, "role_name");
        assert_eq!(sort.direction, None);
    }

    #[test]
    fn parse_filter_shorthand_accepts_common_forms() {
        let eq = parse_filter_shorthand("id = 13").unwrap();
        assert_eq!(eq.column, "id");
        assert!(matches!(eq.op, FilterOp::Eq));
        assert_eq!(eq.value, Some(json!(13)));

        let ilike = parse_filter_shorthand("role_name ilike admin").unwrap();
        assert_eq!(ilike.column, "role_name");
        assert!(matches!(ilike.op, FilterOp::ILike));
        assert_eq!(ilike.value, Some(json!("admin")));

        let is_null = parse_filter_shorthand("deleted_at is null").unwrap();
        assert!(matches!(is_null.op, FilterOp::IsNull));

        let in_values = parse_filter_shorthand("status in [1,2,3]").unwrap();
        assert!(matches!(in_values.op, FilterOp::In));
        assert_eq!(in_values.values, Some(vec![json!(1), json!(2), json!(3)]));

        let between =
            parse_filter_shorthand("create_time between [\"2026-01-01\", \"2026-01-31\"]").unwrap();
        assert_eq!(between.column, "create_time");
        assert!(matches!(between.op, FilterOp::Between));
        assert_eq!(
            between.values,
            Some(vec![json!("2026-01-01"), json!("2026-01-31")])
        );
    }

    #[test]
    fn build_filters_clause_supports_groups_and_between() {
        let schema = TableSchema {
            schema: "public".to_string(),
            table: "sys_role".to_string(),
            comment: None,
            primary_key: vec!["id".to_string()],
            columns: vec![
                test_column("id", "bigint", true, false),
                test_column("status", "smallint", false, false),
                test_column("create_time", "timestamp", false, false),
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        };

        let filters = vec![
            TableFilterInput::Group(TableFilterGroup {
                and: None,
                or: Some(vec![
                    TableFilterInput::Structured(TableFilter {
                        column: "status".to_string(),
                        op: FilterOp::Eq,
                        value: Some(json!(1)),
                        values: None,
                    }),
                    TableFilterInput::Structured(TableFilter {
                        column: "status".to_string(),
                        op: FilterOp::Eq,
                        value: Some(json!(2)),
                        values: None,
                    }),
                ]),
            }),
            TableFilterInput::Structured(TableFilter {
                column: "create_time".to_string(),
                op: FilterOp::Between,
                value: None,
                values: Some(vec![json!("2026-01-01"), json!("2026-01-31")]),
            }),
        ];

        let (clause, params) = build_filters_clause(&schema, Some(&filters)).unwrap();
        let clause = clause.unwrap();
        assert!(
            clause.contains(
                "(\"status\" = CAST($1 AS smallint) OR \"status\" = CAST($2 AS smallint))"
            )
        );
        assert!(
            clause.contains(
                "\"create_time\" BETWEEN CAST($3 AS timestamp) AND CAST($4 AS timestamp)"
            )
        );
        assert_eq!(params.len(), 4);
    }

    fn test_column(
        name: &str,
        pg_type: &str,
        primary_key: bool,
        hidden_on_read: bool,
    ) -> TableColumnSchema {
        TableColumnSchema {
            name: name.to_string(),
            pg_type: pg_type.to_string(),
            nullable: false,
            primary_key,
            hidden_on_read,
            writable_on_create: true,
            writable_on_update: true,
            default_value: None,
            comment: None,
            is_identity: false,
            is_generated: false,
            enum_values: None,
        }
    }
}
