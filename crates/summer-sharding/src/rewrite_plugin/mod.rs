pub mod configurator;
pub mod context;
pub mod helpers;
pub mod registry;

pub use configurator::ShardingRewriteConfigurator;
pub use context::RewriteContext;
pub use registry::PluginRegistry;

use crate::error::Result;

/// SQL 改写插件 trait。
///
/// 实现此 trait 后通过 `.sharding_rewrite_configure()` 在应用层注册，
/// 即可在 SQL Pipeline 中自动执行自定义改写逻辑。
///
/// # 优先级建议
///
/// - `0..50`   — 基础设施级插件（审计、追踪）
/// - `50..100`  — 安全类插件（数据权限、字段过滤）
/// - `100..200` — 业务类插件（自定义条件、表名映射）
/// - `200+`     — 后处理插件（SQL 注释注入等）
///
/// # 示例
///
/// ```rust,ignore
/// use summer_sharding::rewrite_plugin::{SqlRewritePlugin, RewriteContext, helpers};
/// use summer_sharding::error::Result;
///
/// struct DataScopePlugin;
///
/// impl SqlRewritePlugin for DataScopePlugin {
///     fn name(&self) -> &str { "data_scope" }
///     fn order(&self) -> i32 { 50 }
///
///     fn matches(&self, ctx: &RewriteContext) -> bool {
///         ctx.is_select()
///     }
///
///     fn rewrite(&self, ctx: &mut RewriteContext) -> Result<()> {
///         let condition = helpers::build_eq_expr("create_by", "42");
///         helpers::append_where(ctx.statement, condition);
///         Ok(())
///     }
/// }
/// ```
pub trait SqlRewritePlugin: Send + Sync + 'static {
    /// 插件名称，用于日志输出和调试追踪
    fn name(&self) -> &str;

    /// 执行优先级。数字越小越先执行，默认 100。
    fn order(&self) -> i32 {
        100
    }

    /// 判断是否需要对当前 SQL 执行改写。
    /// 返回 `false` 则跳过该插件，不调用 `rewrite`。
    fn matches(&self, ctx: &RewriteContext) -> bool;

    /// 执行改写。通过 `ctx.statement` 直接修改 AST，
    /// 或使用 `helpers` 模块提供的便捷函数操作。
    fn rewrite(&self, ctx: &mut RewriteContext) -> Result<()>;
}
