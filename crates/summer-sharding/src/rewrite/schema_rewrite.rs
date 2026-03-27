use sqlparser::ast::Statement;

use crate::router::QualifiedTableName;

use super::table_rewrite::rewrite_table_names;

pub fn apply_schema_rewrite(
    statement: &mut Statement,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    rewrite_table_names(statement, logic_table, actual_table);
}
