//! 乐观锁插件：UPDATE 自动追加 `version = version + 1` 赋值与 `version = ?` 条件。
//!
//! 触发条件：
//! - SQL 是 UPDATE
//! - 表名命中 `tables` 白名单（空 = 全部表）
//! - 配置的 `version` 列存在于赋值列表之外（不要重复加）
//!
//! 期望调用方在执行前往 [`crate::SqlRewriteContext::extensions`] 注入当前实体的 `version` 旧值，
//! 通过 [`OptimisticLockValue`]；缺失时插件会返回错误。

use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, Expr, Ident, ObjectName,
    Statement as AstStatement, Value as SqlValue,
};

use crate::{
    QualifiedTableName, Result, SqlOperation, SqlRewriteContext, SqlRewriteError, SqlRewritePlugin,
    helpers,
};

use super::matches_name;

/// 当前实体的旧版本号——由业务在请求处理时塞进 extensions。
#[derive(Debug, Clone, Copy)]
pub struct OptimisticLockValue(pub i64);

/// 乐观锁插件配置。
#[derive(Debug, Clone, Default)]
pub struct OptimisticLockConfig {
    /// 版本列名。默认 `version`。
    pub version_column: String,
    /// 只对这些表生效。空 = 全部表。
    pub tables: Vec<QualifiedTableName>,
    /// 始终跳过这些表，优先级高于 `tables`。
    pub skip_tables: Vec<QualifiedTableName>,
}

impl OptimisticLockConfig {
    pub fn new() -> Self {
        Self {
            version_column: "version".to_string(),
            ..Default::default()
        }
    }

    /// 添加一个 SeaORM 实体到白名单。
    pub fn with_entity<E: sea_orm::EntityName + Default>(mut self, _entity: E) -> Self {
        self.tables.push(super::entity_to_qualified(&_entity));
        self
    }

    /// 添加一个字符串表名到白名单（如 `"sys.user"` 或 `"user"`）。
    pub fn with_table(mut self, table: impl AsRef<str>) -> Self {
        self.tables.push(QualifiedTableName::parse(table.as_ref()));
        self
    }

    /// 添加一个 SeaORM 实体到跳过列表。
    pub fn skip_entity<E: sea_orm::EntityName + Default>(mut self, _entity: E) -> Self {
        self.skip_tables.push(super::entity_to_qualified(&_entity));
        self
    }

    /// 添加一个字符串表名到跳过列表。
    pub fn skip_table(mut self, table: impl AsRef<str>) -> Self {
        self.skip_tables
            .push(QualifiedTableName::parse(table.as_ref()));
        self
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
        let candidate = QualifiedTableName::parse(table);
        if self
            .config
            .skip_tables
            .iter()
            .any(|t| matches_name(t, &candidate))
        {
            return false;
        }
        if self.config.tables.is_empty() {
            return true;
        }
        self.config
            .tables
            .iter()
            .any(|t| matches_name(t, &candidate))
    }
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
            let predicate = helpers::build_eq_int_expr(column.as_str(), old_value);
            inject_condition(selection, predicate);
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

        let predicate = helpers::build_eq_int_expr(column.as_str(), old_value);
        inject_condition(selection, predicate);

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

fn inject_condition(selection: &mut Option<Expr>, predicate: Expr) {
    match selection.take() {
        Some(existing) => *selection = Some(helpers::and(existing, predicate)),
        None => *selection = Some(predicate),
    }
}
