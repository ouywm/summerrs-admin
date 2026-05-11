use sqlparser::ast::Statement as AstStatement;

use crate::sql_rewrite::extensions::Extensions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlOperation {
    Select,
    Insert,
    Update,
    Delete,
    Other,
}

pub struct SqlRewriteContext<'a> {
    pub statement: &'a mut AstStatement,
    pub operation: SqlOperation,
    pub tables: Vec<String>,
    pub original_sql: &'a str,
    pub extensions: &'a mut Extensions,
    pub comments: Vec<String>,
}

impl<'a> SqlRewriteContext<'a> {
    pub fn is_select(&self) -> bool {
        self.operation == SqlOperation::Select
    }

    pub fn is_write(&self) -> bool {
        matches!(
            self.operation,
            SqlOperation::Insert | SqlOperation::Update | SqlOperation::Delete
        )
    }

    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }

    pub fn extension_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.extensions.get_mut::<T>()
    }

    pub fn insert_extension<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.extensions.insert(value)
    }

    pub fn get_or_insert_extension<T: Clone + Send + Sync + 'static>(
        &mut self,
        value: T,
    ) -> &mut T {
        self.extensions.get_or_insert(value)
    }

    pub fn append_comment(&mut self, comment: &str) {
        self.comments.push(comment.to_string());
    }

    pub fn primary_table(&self) -> Option<&str> {
        self.tables.first().map(String::as_str)
    }
}
