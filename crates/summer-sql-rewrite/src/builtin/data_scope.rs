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

use sqlparser::ast::{BinaryOperator, Expr, Value as SqlValue};

use crate::{
    QualifiedTableName, Result, SqlOperation, SqlRewriteContext, SqlRewriteError, SqlRewritePlugin,
    helpers,
};

use super::matches_name;

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

#[derive(Debug, Clone, Default)]
pub struct DataScopeConfig {
    /// 用于 `Self_` 范围的列名
    pub creator_column: String,
    /// 用于 `Dept` / `Custom` 范围的列名
    pub dept_column: String,
    /// 只对这些表生效。空 = 全部表。
    pub tables: Vec<QualifiedTableName>,
    /// 始终跳过这些表，优先级高于 `tables`。
    pub skip_tables: Vec<QualifiedTableName>,
}

impl DataScopeConfig {
    pub fn new() -> Self {
        Self {
            creator_column: "creator_id".to_string(),
            dept_column: "dept_id".to_string(),
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
pub struct DataScopePlugin {
    config: DataScopeConfig,
}

impl DataScopePlugin {
    pub fn new(config: DataScopeConfig) -> Self {
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

    fn build_predicate(&self, scope: &DataScope) -> Result<Option<Expr>> {
        match scope {
            DataScope::All => Ok(None),
            DataScope::Self_ { user_id } => {
                Ok(Some(helpers::build_eq_int_expr(&self.config.creator_column, *user_id)))
            }
            DataScope::Custom { dept_ids } => {
                if dept_ids.is_empty() {
                    return Ok(Some(Expr::BinaryOp {
                        left: Box::new(Expr::Value(SqlValue::Number("1".to_string(), false))),
                        op: BinaryOperator::Eq,
                        right: Box::new(Expr::Value(SqlValue::Number("0".to_string(), false))),
                    }));
                }
                Ok(Some(helpers::build_in_int_expr(
                    &self.config.dept_column,
                    dept_ids,
                )))
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
        helpers::append_where(ctx.statement, predicate);
        Ok(())
    }
}
