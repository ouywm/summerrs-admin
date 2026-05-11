//! 数据权限插件：往 SELECT/UPDATE/DELETE 的 WHERE 注入数据范围条件。
//!
//! 业务在请求处理时通过 [`DataScope`] 描述当前用户的数据范围，
//! 插件按范围生成 `creator_id = ?` 或 `dept_id IN (...)` 条件追加到 WHERE 子句。
//!
//! ## 已支持
//! - [`DataScope::All`]：不加任何条件
//! - [`DataScope::Self_`]：注入 `creator_id = ?`
//! - [`DataScope::Custom`]：注入 `dept_id IN (...)`
//!
//! ## TODO（待部门表落地后补全）
//! - [`DataScope::Dept`] / [`DataScope::DeptAndChildren`]

use sqlparser::ast::{BinaryOperator, Expr, Ident, Statement as AstStatement, Value as SqlValue};

use crate::{Result, SqlOperation, SqlRewriteContext, SqlRewriteError, SqlRewritePlugin};

/// 当前请求所允许的数据范围，由业务通过 extensions 注入。
#[derive(Debug, Clone)]
pub enum DataScope {
    /// 不限制（管理员）
    All,
    /// 仅本人创建的数据：注入 `<creator_column> = <user_id>`
    Self_ { user_id: i64 },
    /// 仅本部门：暂未支持，由调用方先用 Custom 列表替代
    Dept { dept_id: i64 },
    /// 本部门 + 子部门：暂未支持，同上
    DeptAndChildren { root_dept_id: i64 },
    /// 自定义部门 ID 列表
    Custom { dept_ids: Vec<i64> },
}

#[derive(Debug, Clone)]
pub struct DataScopeConfig {
    /// 用于 `Self_` 范围的列名
    pub creator_column: String,
    /// 用于 `Dept` / `Custom` 范围的列名
    pub dept_column: String,
    /// 当列表非空时只对这些表生效
    pub include_tables: Vec<String>,
    /// 出现在该列表的表跳过插件
    pub exclude_tables: Vec<String>,
}

impl Default for DataScopeConfig {
    fn default() -> Self {
        Self {
            creator_column: "creator_id".to_string(),
            dept_column: "dept_id".to_string(),
            include_tables: Vec::new(),
            exclude_tables: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct DataScopePlugin {
    config: DataScopeConfig,
}

impl DataScopePlugin {
    pub fn new(config: DataScopeConfig) -> Self {
        Self { config }
    }

    fn applies_to_table(&self, table: &str) -> bool {
        let table_low = table.to_ascii_lowercase();
        let trimmed = table_low.rsplit('.').next().unwrap_or(table_low.as_str());
        if self
            .config
            .exclude_tables
            .iter()
            .any(|t| eq_or_short(t.as_str(), table_low.as_str(), trimmed))
        {
            return false;
        }
        if self.config.include_tables.is_empty() {
            return true;
        }
        self.config
            .include_tables
            .iter()
            .any(|t| eq_or_short(t.as_str(), table_low.as_str(), trimmed))
    }

    fn build_predicate(&self, scope: &DataScope) -> Result<Option<Expr>> {
        match scope {
            DataScope::All => Ok(None),
            DataScope::Self_ { user_id } => Ok(Some(Expr::BinaryOp {
                left: Box::new(Expr::Identifier(Ident::new(self.config.creator_column.as_str()))),
                op: BinaryOperator::Eq,
                right: Box::new(Expr::Value(SqlValue::Number(user_id.to_string(), false))),
            })),
            DataScope::Custom { dept_ids } => {
                if dept_ids.is_empty() {
                    return Ok(Some(Expr::BinaryOp {
                        left: Box::new(Expr::Value(SqlValue::Number("1".to_string(), false))),
                        op: BinaryOperator::Eq,
                        right: Box::new(Expr::Value(SqlValue::Number("0".to_string(), false))),
                    }));
                }
                let list = dept_ids
                    .iter()
                    .map(|id| Expr::Value(SqlValue::Number(id.to_string(), false)))
                    .collect();
                Ok(Some(Expr::InList {
                    expr: Box::new(Expr::Identifier(Ident::new(self.config.dept_column.as_str()))),
                    list,
                    negated: false,
                }))
            }
            DataScope::Dept { .. } | DataScope::DeptAndChildren { .. } => {
                Err(SqlRewriteError::Plugin {
                    plugin: "data_scope".to_string(),
                    message: "DataScope::Dept / DeptAndChildren not yet implemented; use DataScope::Custom".to_string(),
                })
            }
        }
    }
}

fn eq_or_short(pattern: &str, full: &str, short: &str) -> bool {
    let p = pattern.to_ascii_lowercase();
    p == full || p == short
}

impl SqlRewritePlugin for DataScopePlugin {
    fn name(&self) -> &str {
        "data_scope"
    }

    fn order(&self) -> i32 {
        70
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        if !matches!(
            ctx.operation,
            SqlOperation::Select | SqlOperation::Update | SqlOperation::Delete
        ) {
            return false;
        }
        if ctx.extension::<DataScope>().is_none() {
            return false;
        }
        ctx.tables.iter().any(|t| self.applies_to_table(t.as_str()))
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let scope = match ctx.extension::<DataScope>() {
            Some(s) => s.clone(),
            None => return Ok(()),
        };
        let Some(predicate) = self.build_predicate(&scope)? else {
            return Ok(());
        };

        match ctx.statement {
            AstStatement::Query(query) => apply_to_query(query, predicate),
            AstStatement::Update { selection, .. } => apply_to_selection(selection, predicate),
            AstStatement::Delete(delete) => apply_to_selection(&mut delete.selection, predicate),
            _ => {}
        }
        Ok(())
    }
}

fn apply_to_query(query: &mut sqlparser::ast::Query, predicate: Expr) {
    if let sqlparser::ast::SetExpr::Select(select) = query.body.as_mut() {
        apply_to_selection(&mut select.selection, predicate);
    }
}

fn apply_to_selection(selection: &mut Option<Expr>, predicate: Expr) {
    match selection.take() {
        Some(existing) => {
            *selection = Some(Expr::BinaryOp {
                left: Box::new(existing),
                op: BinaryOperator::And,
                right: Box::new(predicate),
            });
        }
        None => *selection = Some(predicate),
    }
}
