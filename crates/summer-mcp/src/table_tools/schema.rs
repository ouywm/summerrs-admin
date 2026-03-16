use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use sea_orm::{
    DatabaseConnection, DbBackend, FromQueryResult, JsonValue, SelectModel, SelectorRaw, Statement,
    Value,
};
use serde::{Deserialize, Serialize};

use crate::{
    error_model::{internal_error, invalid_params_error},
    tools::support::error_chain_message,
};

const PUBLIC_SCHEMA: &str = "public";
const EXCLUDED_TABLES: &[&str] = &["seaql_migrations"];
const HIDDEN_READ_COLUMN_NAMES: &[&str] = &[
    "password",
    "password_hash",
    "passwd",
    "salt",
    "secret",
    "access_token",
    "refresh_token",
];
const BLOCKED_WRITE_COLUMN_NAMES: &[&str] = &["create_time", "update_time"];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TableColumnSchema {
    pub name: String,
    pub pg_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub hidden_on_read: bool,
    pub writable_on_create: bool,
    pub writable_on_update: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_identity: bool,
    pub is_generated: bool,
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TableIndexSchema {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TableForeignKeySchema {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_schema: String,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TableCheckConstraintSchema {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TableSchema {
    pub schema: String,
    pub table: String,
    pub comment: Option<String>,
    pub primary_key: Vec<String>,
    pub columns: Vec<TableColumnSchema>,
    pub indexes: Vec<TableIndexSchema>,
    pub foreign_keys: Vec<TableForeignKeySchema>,
    pub check_constraints: Vec<TableCheckConstraintSchema>,
}

impl TableSchema {
    pub fn qualified_name(&self) -> String {
        format!(
            "{}.{}",
            quote_identifier(&self.schema),
            quote_identifier(&self.table)
        )
    }

    pub fn column(&self, name: &str) -> Option<&TableColumnSchema> {
        self.columns.iter().find(|column| column.name == name)
    }

    pub fn primary_key_columns(&self) -> Vec<&TableColumnSchema> {
        self.columns
            .iter()
            .filter(|column| column.primary_key)
            .collect()
    }

    pub fn readable_columns(&self) -> Vec<&TableColumnSchema> {
        self.columns
            .iter()
            .filter(|column| !column.hidden_on_read)
            .collect()
    }
}

#[derive(Debug, FromQueryResult)]
struct TableNameRow {
    table_name: String,
}

#[derive(Debug, FromQueryResult)]
struct ColumnSchemaRow {
    table_comment: Option<String>,
    column_name: String,
    pg_type: String,
    is_nullable: bool,
    column_default: Option<String>,
    is_identity: bool,
    is_generated: bool,
    is_primary_key: bool,
    column_comment: Option<String>,
    enum_values: Option<JsonValue>,
}

#[derive(Debug, FromQueryResult)]
struct IndexSchemaRow {
    index_name: String,
    is_unique: bool,
    is_primary: bool,
    columns: JsonValue,
}

#[derive(Debug, FromQueryResult)]
struct ForeignKeySchemaRow {
    constraint_name: String,
    columns: JsonValue,
    referenced_schema: String,
    referenced_table: String,
    referenced_columns: JsonValue,
    on_update: String,
    on_delete: String,
}

#[derive(Debug, FromQueryResult)]
struct CheckConstraintRow {
    constraint_name: String,
    expression: String,
}

pub async fn list_tables(db: &DatabaseConnection) -> Result<Vec<String>, McpError> {
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
            SELECT c.relname AS table_name
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1
              AND c.relkind = 'r'
            ORDER BY c.relname
        "#,
        [Value::from(PUBLIC_SCHEMA.to_string())],
    );

    let rows = SelectorRaw::<SelectModel<TableNameRow>>::from_statement::<TableNameRow>(statement)
        .all(db)
        .await
        .map_err(|error| db_error("list public tables", error))?;

    let tables: Vec<String> = rows
        .into_iter()
        .map(|row: TableNameRow| row.table_name)
        .filter(|table: &String| !EXCLUDED_TABLES.contains(&table.as_str()))
        .collect();

    Ok(tables)
}

pub async fn describe_table(db: &DatabaseConnection, table: &str) -> Result<TableSchema, McpError> {
    ensure_valid_identifier(table, "table")?;

    let (comment, columns, primary_key) = load_columns_and_primary_key(db, table).await?;

    let indexes = load_indexes(db, table).await?;
    let foreign_keys = load_foreign_keys(db, table).await?;
    let check_constraints = load_check_constraints(db, table).await?;

    Ok(TableSchema {
        schema: PUBLIC_SCHEMA.to_string(),
        table: table.to_string(),
        comment,
        primary_key,
        columns,
        indexes,
        foreign_keys,
        check_constraints,
    })
}

/// Lightweight variant of [`describe_table`] that only loads columns and primary key,
/// skipping indexes, foreign keys, and check constraints. Suitable for CRUD operations
/// where only column metadata is needed.
pub async fn describe_table_for_crud(
    db: &DatabaseConnection,
    table: &str,
) -> Result<TableSchema, McpError> {
    ensure_valid_identifier(table, "table")?;

    let (comment, columns, primary_key) = load_columns_and_primary_key(db, table).await?;

    Ok(TableSchema {
        schema: PUBLIC_SCHEMA.to_string(),
        table: table.to_string(),
        comment,
        primary_key,
        columns,
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
    })
}

async fn load_columns_and_primary_key(
    db: &DatabaseConnection,
    table: &str,
) -> Result<(Option<String>, Vec<TableColumnSchema>, Vec<String>), McpError> {
    let column_rows = load_column_rows(db, table).await?;
    if column_rows.is_empty() {
        return Err(invalid_params_error(
            "table_not_found",
            "Table not found",
            Some("Read schema://tables first to confirm the live table name."),
            Some(format!("unknown table `{table}`")),
            Some(serde_json::json!({ "table": table })),
        ));
    }

    let comment = column_rows
        .first()
        .and_then(|row| row.table_comment.clone())
        .filter(|value| !value.trim().is_empty());

    let columns = column_rows
        .into_iter()
        .map(|row| {
            let hidden_on_read = HIDDEN_READ_COLUMN_NAMES.contains(&row.column_name.as_str());
            let generated_by_sequence = row
                .column_default
                .as_deref()
                .is_some_and(|value: &str| value.starts_with("nextval("));
            let generated = row.is_identity || row.is_generated || generated_by_sequence;
            let blocked_by_name = BLOCKED_WRITE_COLUMN_NAMES.contains(&row.column_name.as_str());

            Ok(TableColumnSchema {
                name: row.column_name.clone(),
                pg_type: row.pg_type,
                nullable: row.is_nullable,
                primary_key: row.is_primary_key,
                hidden_on_read,
                writable_on_create: !generated && !blocked_by_name,
                writable_on_update: !row.is_primary_key && !generated && !blocked_by_name,
                default_value: row.column_default,
                comment: row.column_comment.filter(|value| !value.trim().is_empty()),
                is_identity: row.is_identity,
                is_generated: row.is_generated,
                enum_values: match row.enum_values {
                    Some(value) => Some(json_array_to_strings(value, "column enum_values")?),
                    None => None,
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let primary_key = columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();

    Ok((comment, columns, primary_key))
}

async fn load_column_rows(
    db: &DatabaseConnection,
    table: &str,
) -> Result<Vec<ColumnSchemaRow>, McpError> {
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
            SELECT
                obj_description(c.oid, 'pg_class') AS table_comment,
                a.attname AS column_name,
                pg_catalog.format_type(a.atttypid, a.atttypmod) AS pg_type,
                NOT a.attnotnull AS is_nullable,
                pg_get_expr(ad.adbin, ad.adrelid) AS column_default,
                (a.attidentity <> '') AS is_identity,
                (a.attgenerated <> '') AS is_generated,
                EXISTS (
                    SELECT 1
                    FROM pg_index i
                    WHERE i.indrelid = c.oid
                      AND i.indisprimary
                      AND a.attnum = ANY(i.indkey)
                ) AS is_primary_key,
                col_description(c.oid, a.attnum) AS column_comment,
                CASE
                    WHEN t.typtype = 'e' THEN (
                        SELECT json_agg(e.enumlabel ORDER BY e.enumsortorder)
                        FROM pg_enum e
                        WHERE e.enumtypid = t.oid
                    )
                    ELSE NULL
                END AS enum_values
            FROM pg_attribute a
            JOIN pg_class c ON a.attrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_type t ON t.oid = a.atttypid
            LEFT JOIN pg_attrdef ad ON ad.adrelid = c.oid AND ad.adnum = a.attnum
            WHERE n.nspname = $1
              AND c.relname = $2
              AND c.relkind = 'r'
              AND a.attnum > 0
              AND NOT a.attisdropped
            ORDER BY a.attnum
        "#,
        [
            Value::from(PUBLIC_SCHEMA.to_string()),
            Value::from(table.to_string()),
        ],
    );

    SelectorRaw::<SelectModel<ColumnSchemaRow>>::from_statement::<ColumnSchemaRow>(statement)
        .all(db)
        .await
        .map_err(|error| db_error(format!("describe table `{table}` columns"), error))
}

async fn load_indexes(
    db: &DatabaseConnection,
    table: &str,
) -> Result<Vec<TableIndexSchema>, McpError> {
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
            SELECT
                idx.relname AS index_name,
                ix.indisunique AS is_unique,
                ix.indisprimary AS is_primary,
                COALESCE(
                    json_agg(att.attname ORDER BY ord.ordinality)
                    FILTER (WHERE att.attname IS NOT NULL),
                    '[]'::json
                ) AS columns
            FROM pg_class tbl
            JOIN pg_namespace ns ON ns.oid = tbl.relnamespace
            JOIN pg_index ix ON ix.indrelid = tbl.oid
            JOIN pg_class idx ON idx.oid = ix.indexrelid
            LEFT JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS ord(attnum, ordinality) ON TRUE
            LEFT JOIN pg_attribute att ON att.attrelid = tbl.oid AND att.attnum = ord.attnum
            WHERE ns.nspname = $1
              AND tbl.relname = $2
              AND tbl.relkind = 'r'
            GROUP BY idx.relname, ix.indisunique, ix.indisprimary
            ORDER BY ix.indisprimary DESC, ix.indisunique DESC, idx.relname
        "#,
        [
            Value::from(PUBLIC_SCHEMA.to_string()),
            Value::from(table.to_string()),
        ],
    );

    let rows =
        SelectorRaw::<SelectModel<IndexSchemaRow>>::from_statement::<IndexSchemaRow>(statement)
            .all(db)
            .await
            .map_err(|error| db_error(format!("describe table `{table}` indexes"), error))?;

    rows.into_iter()
        .map(|row| {
            Ok(TableIndexSchema {
                name: row.index_name,
                columns: json_array_to_strings(row.columns, "index columns")?,
                unique: row.is_unique,
                primary: row.is_primary,
            })
        })
        .collect()
}

async fn load_foreign_keys(
    db: &DatabaseConnection,
    table: &str,
) -> Result<Vec<TableForeignKeySchema>, McpError> {
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
            SELECT
                con.conname AS constraint_name,
                COALESCE(json_agg(src.attname ORDER BY ord.ordinality), '[]'::json) AS columns,
                target_ns.nspname AS referenced_schema,
                target.relname AS referenced_table,
                COALESCE(json_agg(dst.attname ORDER BY ord.ordinality), '[]'::json) AS referenced_columns,
                CASE con.confupdtype
                    WHEN 'a' THEN 'no_action'
                    WHEN 'r' THEN 'restrict'
                    WHEN 'c' THEN 'cascade'
                    WHEN 'n' THEN 'set_null'
                    WHEN 'd' THEN 'set_default'
                    ELSE 'no_action'
                END AS on_update,
                CASE con.confdeltype
                    WHEN 'a' THEN 'no_action'
                    WHEN 'r' THEN 'restrict'
                    WHEN 'c' THEN 'cascade'
                    WHEN 'n' THEN 'set_null'
                    WHEN 'd' THEN 'set_default'
                    ELSE 'no_action'
                END AS on_delete
            FROM pg_constraint con
            JOIN pg_class source ON source.oid = con.conrelid
            JOIN pg_namespace source_ns ON source_ns.oid = source.relnamespace
            JOIN pg_class target ON target.oid = con.confrelid
            JOIN pg_namespace target_ns ON target_ns.oid = target.relnamespace
            JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY
                AS ord(src_attnum, dst_attnum, ordinality) ON TRUE
            JOIN pg_attribute src ON src.attrelid = source.oid AND src.attnum = ord.src_attnum
            JOIN pg_attribute dst ON dst.attrelid = target.oid AND dst.attnum = ord.dst_attnum
            WHERE con.contype = 'f'
              AND source_ns.nspname = $1
              AND source.relname = $2
            GROUP BY
                con.conname,
                target_ns.nspname,
                target.relname,
                con.confupdtype,
                con.confdeltype
            ORDER BY con.conname
        "#,
        [
            Value::from(PUBLIC_SCHEMA.to_string()),
            Value::from(table.to_string()),
        ],
    );

    let rows =
        SelectorRaw::<SelectModel<ForeignKeySchemaRow>>::from_statement::<ForeignKeySchemaRow>(
            statement,
        )
        .all(db)
        .await
        .map_err(|error| db_error(format!("describe table `{table}` foreign keys"), error))?;

    rows.into_iter()
        .map(|row| {
            Ok(TableForeignKeySchema {
                name: row.constraint_name,
                columns: json_array_to_strings(row.columns, "foreign key columns")?,
                referenced_schema: row.referenced_schema,
                referenced_table: row.referenced_table,
                referenced_columns: json_array_to_strings(
                    row.referenced_columns,
                    "foreign key referenced_columns",
                )?,
                on_update: row.on_update,
                on_delete: row.on_delete,
            })
        })
        .collect()
}

async fn load_check_constraints(
    db: &DatabaseConnection,
    table: &str,
) -> Result<Vec<TableCheckConstraintSchema>, McpError> {
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
            SELECT
                con.conname AS constraint_name,
                pg_get_constraintdef(con.oid, true) AS expression
            FROM pg_constraint con
            JOIN pg_class c ON c.oid = con.conrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE con.contype = 'c'
              AND n.nspname = $1
              AND c.relname = $2
            ORDER BY con.conname
        "#,
        [
            Value::from(PUBLIC_SCHEMA.to_string()),
            Value::from(table.to_string()),
        ],
    );

    let rows =
        SelectorRaw::<SelectModel<CheckConstraintRow>>::from_statement::<CheckConstraintRow>(
            statement,
        )
        .all(db)
        .await
        .map_err(|error| db_error(format!("describe table `{table}` check constraints"), error))?;

    Ok(rows
        .into_iter()
        .map(|row| TableCheckConstraintSchema {
            name: row.constraint_name,
            expression: row.expression,
        })
        .collect())
}

fn json_array_to_strings(value: JsonValue, field: &str) -> Result<Vec<String>, McpError> {
    let Some(items) = value.as_array() else {
        return Err(internal_error(
            "serialization_failed",
            "Serialization failed",
            Some("Check that the PostgreSQL metadata query returns a JSON array."),
            Some(format!("expected `{field}` to be a JSON array")),
            Some(serde_json::json!({ "field": field })),
        ));
    };

    items
        .iter()
        .map(|item| {
            item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                internal_error(
                    "serialization_failed",
                    "Serialization failed",
                    Some("Check that the PostgreSQL metadata query returns string array items."),
                    Some(format!("expected `{field}` to contain only strings")),
                    Some(serde_json::json!({ "field": field })),
                )
            })
        })
        .collect()
}

pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub fn readable_select_list(
    schema: &TableSchema,
    requested: Option<&[String]>,
) -> Result<String, McpError> {
    let columns = resolve_readable_columns(schema, requested)?;
    Ok(columns
        .into_iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", "))
}

pub fn resolve_readable_columns<'a>(
    schema: &'a TableSchema,
    requested: Option<&[String]>,
) -> Result<Vec<&'a TableColumnSchema>, McpError> {
    let columns = if let Some(requested) = requested {
        if requested.is_empty() {
            return Err(invalid_params_error(
                "invalid_columns",
                "Invalid columns",
                Some(
                    "Pass at least one requested column, or omit the columns field to read all readable columns.",
                ),
                Some("requested columns cannot be empty".to_string()),
                None,
            ));
        }

        let mut resolved = Vec::with_capacity(requested.len());
        for column_name in requested {
            let Some(column) = schema.column(column_name) else {
                return Err(invalid_params_error(
                    "column_not_found",
                    "Column not found",
                    Some(
                        "Read schema://table/{table} first to confirm readable and writable columns.",
                    ),
                    Some(format!(
                        "unknown column `{column_name}` for table `{}`",
                        schema.table
                    )),
                    Some(serde_json::json!({
                        "table": schema.table,
                        "column": column_name,
                    })),
                ));
            };
            if column.hidden_on_read {
                return Err(invalid_params_error(
                    "hidden_column_access",
                    "Hidden column cannot be accessed",
                    Some(
                        "Use the schema metadata to avoid hidden columns such as passwords or blocked audit fields.",
                    ),
                    Some(format!(
                        "column `{column_name}` on table `{}` is hidden from reads",
                        schema.table
                    )),
                    Some(serde_json::json!({
                        "table": schema.table,
                        "column": column_name,
                    })),
                ));
            }
            resolved.push(column);
        }
        resolved
    } else {
        schema.readable_columns()
    };

    if columns.is_empty() {
        return Err(invalid_params_error(
            "no_readable_columns",
            "No readable columns available",
            Some("Review hidden column rules or read the full table schema first."),
            Some(format!("table `{}` has no readable columns", schema.table)),
            Some(serde_json::json!({ "table": schema.table })),
        ));
    }

    Ok(columns)
}

pub fn ensure_valid_identifier(identifier: &str, kind: &str) -> Result<(), McpError> {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_params_error(
            "invalid_identifier",
            "Invalid identifier",
            Some(
                "Use ASCII letters, digits, and underscores only, and start with a letter or underscore.",
            ),
            Some(format!("{kind} identifier cannot be empty")),
            Some(serde_json::json!({ "kind": kind })),
        ));
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(invalid_params_error(
            "invalid_identifier",
            "Invalid identifier",
            Some(
                "Use ASCII letters, digits, and underscores only, and start with a letter or underscore.",
            ),
            Some(format!("{kind} identifier `{identifier}` is invalid")),
            Some(serde_json::json!({ "kind": kind, "identifier": identifier })),
        ));
    }

    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(invalid_params_error(
            "invalid_identifier",
            "Invalid identifier",
            Some(
                "Use ASCII letters, digits, and underscores only, and start with a letter or underscore.",
            ),
            Some(format!("{kind} identifier `{identifier}` is invalid")),
            Some(serde_json::json!({ "kind": kind, "identifier": identifier })),
        ));
    }

    Ok(())
}

pub fn bind_cast_value(
    placeholder_index: usize,
    column: &TableColumnSchema,
    value: &JsonValue,
    params: &mut Vec<Value>,
) -> Result<String, McpError> {
    let Some(bound) = json_value_to_bound_string(value)? else {
        return Ok("NULL".to_string());
    };
    params.push(Value::from(bound));
    Ok(format!("CAST(${placeholder_index} AS {})", column.pg_type))
}

fn json_value_to_bound_string(value: &JsonValue) -> Result<Option<String>, McpError> {
    Ok(match value {
        JsonValue::Null => None,
        JsonValue::Bool(value) => Some(value.to_string()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            Some(serde_json::to_string(value).map_err(|error| {
                internal_error(
                    "serialization_failed",
                    "Serialization failed",
                    Some("Check that the bound JSON value can be encoded to a string."),
                    Some(error.to_string()),
                    None,
                )
            })?)
        }
    })
}

pub(crate) fn db_error(action: impl Into<String>, error: sea_orm::DbErr) -> McpError {
    let action = action.into();
    internal_error(
        "database_operation_failed",
        "Database operation failed",
        Some("Check the target schema, live table structure, and database connectivity."),
        Some(format!(
            "failed to {action}: {}",
            error_chain_message(&error)
        )),
        Some(serde_json::json!({ "action": action })),
    )
}
