use sqlparser::ast::Statement;

use crate::{
    connector::{ShardingAccessContext, statement::StatementContext},
    router::{RoutePlan, RouteTarget, SqlOperation},
};

/// 插件改写上下文。
///
/// 每个 `RouteTarget`（即每个物理分片）会生成一个独立的 `RewriteContext`，
/// 插件对其修改只影响该分片的 SQL。
pub struct RewriteContext<'a> {
    /// AST 本体（可变引用），插件可直接修改
    pub statement: &'a mut Statement,

    /// SQL 解析出的元信息（操作类型、涉及的表名、列名、条件等）
    pub analysis: &'a StatementContext,

    /// 完整路由计划
    pub route: &'a RoutePlan,

    /// 当前正在处理的路由目标（物理分片）
    pub target: &'a RouteTarget,

    /// 请求级上下文（用户角色、权限、extensions 等）
    /// 如果调用方未设置则为 None
    pub access_ctx: Option<&'a ShardingAccessContext>,

    /// 审计注释列表。
    /// 插件通过 `append_comment()` 追加注释，由 `DefaultSqlRewriter` 在
    /// `statement.to_string()` 后通过 `helpers::format_with_comments()` 拼接到 SQL 末尾。
    pub comments: Vec<String>,
}

impl<'a> RewriteContext<'a> {
    /// 当前 SQL 操作类型
    pub fn operation(&self) -> SqlOperation {
        self.analysis.operation
    }

    /// 当前目标数据源名称
    pub fn datasource(&self) -> &str {
        &self.target.datasource
    }

    /// 当前目标的主物理表名（取第一个 table_rewrite 的 actual_table）
    pub fn current_table(&self) -> Option<&str> {
        self.target
            .table_rewrites
            .first()
            .map(|rw| rw.actual_table.table.as_str())
    }

    /// 当前目标的主逻辑表名
    pub fn logic_table(&self) -> Option<&str> {
        self.target
            .table_rewrites
            .first()
            .map(|rw| rw.logic_table.table.as_str())
    }

    /// 便捷方法：从 extensions 中获取指定类型的数据
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.access_ctx.and_then(|ctx| ctx.extensions.get::<T>())
    }

    /// 是否为 SELECT 查询
    pub fn is_select(&self) -> bool {
        self.analysis.operation == SqlOperation::Select
    }

    /// 是否为写操作（INSERT/UPDATE/DELETE）
    pub fn is_write(&self) -> bool {
        matches!(
            self.analysis.operation,
            SqlOperation::Insert | SqlOperation::Update | SqlOperation::Delete
        )
    }

    /// 是否为多分片扇出查询
    pub fn is_fanout(&self) -> bool {
        self.route.targets.len() > 1
    }

    /// 追加审计注释。
    ///
    /// 注释会在 SQL 渲染后以 `/* ... */` 形式拼接到末尾。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// ctx.append_comment(&format!("user_id={}, ds={}", user.id, ctx.datasource()));
    /// ```
    pub fn append_comment(&mut self, comment: &str) {
        self.comments.push(comment.to_string());
    }
}
