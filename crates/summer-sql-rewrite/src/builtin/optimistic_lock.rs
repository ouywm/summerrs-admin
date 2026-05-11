//! 乐观锁插件：UPDATE 自动追加 `version = version + 1` 赋值与 `version = ?` 条件。
//!
//! 触发条件：
//! - SQL 是 UPDATE
//! - 表名命中插件配置（默认全部表，可白名单/黑名单约束）
//! - 配置的 `version` 列存在于赋值列表之外（不要重复加）
//!
//! 期望调用方在执行前往 [`crate::SqlRewriteContext::extensions`] 注入当前实体的 `version` 旧值，
//! 通过 [`OptimisticLockValue`]；缺失时插件会返回错误。

use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, Expr, Ident, ObjectName,
    Statement as AstStatement, Value as SqlValue,
};

use crate::{Result, SqlOperation, SqlRewriteContext, SqlRewriteError, SqlRewritePlugin};

/// 当前实体的旧版本号——由业务在请求处理时塞进 extensions。
#[derive(Debug, Clone, Copy)]
pub struct OptimisticLockValue(pub i64);

/// 乐观锁插件配置。
#[derive(Debug, Clone)]
pub struct OptimisticLockConfig {
    /// 版本列名。默认 `version`。
    pub version_column: String,
    /// 当列表非空时只对这些表生效。
    pub include_tables: Vec<String>,
    /// 出现在该列表的表跳过插件。
    pub exclude_tables: Vec<String>,
}

impl Default for OptimisticLockConfig {
    fn default() -> Self {
        Self {
            version_column: "version".to_string(),
            include_tables: Vec::new(),
            exclude_tables: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct OptimisticLockPlugin {
    config: OptimisticLockConfig,
}

impl OptimisticLockPlugin {
    pub fn new(config: OptimisticLockConfig) -> Self {
        Self { config }
    }

    fn applies_to_table(&self, table: &str) -> bool {
        let table_low = table.to_ascii_lowercase();
        let trimmed = table_low.rsplit('.').next().unwrap_or(table_low.as_str());
        if self
            .config
            .exclude_tables
            .iter()
            .any(|t| matches_table(t.as_str(), table_low.as_str(), trimmed))
        {
            return false;
        }
        if self.config.include_tables.is_empty() {
            return true;
        }
        self.config
            .include_tables
            .iter()
            .any(|t| matches_table(t.as_str(), table_low.as_str(), trimmed))
    }
}

fn matches_table(pattern: &str, full: &str, short: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    pattern == full || pattern == short
}

impl SqlRewritePlugin for OptimisticLockPlugin {
    fn name(&self) -> &str {
        "optimistic_lock"
    }

    fn order(&self) -> i32 {
        50
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        if ctx.operation != SqlOperation::Update {
            return false;
        }
        ctx.tables
            .iter()
            .any(|table| self.applies_to_table(table.as_str()))
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let column = self.config.version_column.clone();
        let old_value = ctx
            .extension::<OptimisticLockValue>()
            .map(|v| v.0)
            .unwrap_or(-1);

        let AstStatement::Update {
            assignments,
            selection,
            ..
        } = ctx.statement
        else {
            return Ok(());
        };

        if assignments
            .iter()
            .any(|a| assignment_targets_column(a, column.as_str()))
        {
            inject_where_version_eq(selection, column.as_str(), old_value);
            return Ok(());
        }

        assignments.push(Assignment {
            target: AssignmentTarget::ColumnName(ObjectName(vec![Ident::new(column.as_str())])),
            value: Expr::BinaryOp {
                left: Box::new(Expr::Identifier(Ident::new(column.as_str()))),
                op: BinaryOperator::Plus,
                right: Box::new(Expr::Value(SqlValue::Number("1".to_string(), false))),
            },
        });

        inject_where_version_eq(selection, column.as_str(), old_value);

        ctx.append_comment(format!("optimistic_lock:version={old_value}").as_str());

        if old_value < 0 {
            return Err(SqlRewriteError::Plugin {
                plugin: "optimistic_lock".to_string(),
                message: format!(
                    "UPDATE on table requires `OptimisticLockValue` extension; column `{column}` was not provided"
                ),
            });
        }
        Ok(())
    }
}

fn assignment_targets_column(a: &Assignment, column: &str) -> bool {
    if let AssignmentTarget::ColumnName(name) = &a.target {
        return name
            .0
            .last()
            .map(|i| i.value.eq_ignore_ascii_case(column))
            .unwrap_or(false);
    }
    false
}

fn inject_where_version_eq(selection: &mut Option<Expr>, column: &str, old_value: i64) {
    let predicate = Expr::BinaryOp {
        left: Box::new(Expr::Identifier(Ident::new(column))),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(SqlValue::Number(old_value.to_string(), false))),
    };
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
