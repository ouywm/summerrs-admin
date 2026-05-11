//! 自动填充审计字段插件。
//!
//! 对所有命中的表，自动给 INSERT/UPDATE 追加：
//! - INSERT: `create_time` / `create_by` / `update_time` / `update_by`
//! - UPDATE: `update_time` / `update_by`
//!
//! 调用方需要在请求级 [`crate::Extensions`] 中注入 [`CurrentUser`]（一般由 web 中间件做）。
//! 如果用户上下文缺失，插件会跳过填充用户字段，只填时间。

use chrono::Local;
use sqlparser::ast::{
    Assignment, AssignmentTarget, Expr, Ident, ObjectName, Statement as AstStatement,
    Value as SqlValue,
};

use crate::{Result, SqlOperation, SqlRewriteContext, SqlRewritePlugin};

/// 当前操作用户上下文，由业务通过 extensions 注入。
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: i64,
    pub user_name: String,
}

/// 自动填充字段名配置。空字符串表示禁用该字段。
#[derive(Debug, Clone)]
pub struct AutoFillConfig {
    pub create_time_column: String,
    pub create_by_column: String,
    pub update_time_column: String,
    pub update_by_column: String,
    /// 只对这些表生效。空 = 全部表。
    pub tables: Vec<String>,
    /// `create_by` / `update_by` 写用户 ID 还是用户名。
    pub use_user_id_for_by: bool,
}

impl Default for AutoFillConfig {
    fn default() -> Self {
        Self {
            create_time_column: "create_time".to_string(),
            create_by_column: "create_by".to_string(),
            update_time_column: "update_time".to_string(),
            update_by_column: "update_by".to_string(),
            tables: Vec::new(),
            use_user_id_for_by: true,
        }
    }
}

#[derive(Debug)]
pub struct AutoFillPlugin {
    config: AutoFillConfig,
}

impl AutoFillPlugin {
    pub fn new(config: AutoFillConfig) -> Self {
        Self { config }
    }

    fn applies_to_table(&self, table: &str) -> bool {
        if self.config.tables.is_empty() {
            return true;
        }
        let table_low = table.to_ascii_lowercase();
        let short = table_low.rsplit('.').next().unwrap_or(table_low.as_str());
        self.config
            .tables
            .iter()
            .any(|t| eq_or_short(t.as_str(), table_low.as_str(), short))
    }

    fn now_literal(&self) -> Expr {
        Expr::Value(SqlValue::SingleQuotedString(
            Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ))
    }

    fn user_by_literal(&self, user: &CurrentUser) -> Expr {
        if self.config.use_user_id_for_by {
            Expr::Value(SqlValue::Number(user.user_id.to_string(), false))
        } else {
            Expr::Value(SqlValue::SingleQuotedString(user.user_name.clone()))
        }
    }
}

fn eq_or_short(pattern: &str, full: &str, short: &str) -> bool {
    let p = pattern.to_ascii_lowercase();
    p == full || p == short
}

impl SqlRewritePlugin for AutoFillPlugin {
    fn name(&self) -> &str {
        "auto_fill"
    }

    fn order(&self) -> i32 {
        60
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        if !matches!(ctx.operation, SqlOperation::Insert | SqlOperation::Update) {
            return false;
        }
        ctx.tables.iter().any(|t| self.applies_to_table(t.as_str()))
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let user = ctx.extension::<CurrentUser>().cloned();
        match ctx.statement {
            AstStatement::Insert(insert) => {
                let now = self.now_literal();
                push_insert_column(insert, self.config.create_time_column.as_str(), now.clone());
                push_insert_column(insert, self.config.update_time_column.as_str(), now);
                if let Some(user) = &user {
                    let by = self.user_by_literal(user);
                    push_insert_column(insert, self.config.create_by_column.as_str(), by.clone());
                    push_insert_column(insert, self.config.update_by_column.as_str(), by);
                }
            }
            AstStatement::Update { assignments, .. } => {
                let now = self.now_literal();
                push_assignment(assignments, self.config.update_time_column.as_str(), now);
                if let Some(user) = user {
                    let by = self.user_by_literal(&user);
                    push_assignment(assignments, self.config.update_by_column.as_str(), by);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn push_assignment(assignments: &mut Vec<Assignment>, column: &str, value: Expr) {
    if column.is_empty() {
        return;
    }
    let already_set = assignments.iter().any(|a| {
        matches!(&a.target, AssignmentTarget::ColumnName(n)
            if n.0.last().map(|i| i.value.eq_ignore_ascii_case(column)).unwrap_or(false))
    });
    if already_set {
        return;
    }
    assignments.push(Assignment {
        target: AssignmentTarget::ColumnName(ObjectName(vec![Ident::new(column)])),
        value,
    });
}

fn push_insert_column(insert: &mut sqlparser::ast::Insert, column: &str, value: Expr) {
    if column.is_empty() {
        return;
    }
    let already_present = insert
        .columns
        .iter()
        .any(|c| c.value.eq_ignore_ascii_case(column));
    if already_present {
        return;
    }
    insert.columns.push(Ident::new(column));
    let Some(source) = insert.source.as_deref_mut() else {
        return;
    };
    if let sqlparser::ast::SetExpr::Values(values) = source.body.as_mut() {
        for row in &mut values.rows {
            row.push(value.clone());
        }
    }
}
