use crate::sql_rewrite::{QualifiedTableName, context::SqlRewriteContext, error::Result};

pub trait SqlRewritePlugin: Send + Sync + 'static {
    /// 插件名字
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// 插件执行顺序
    fn order(&self) -> i32 {
        100
    }

    /// 只对这些表生效。空 = 全部表。由框架在调用 `matches()` 前自动检查。
    fn tables(&self) -> &[QualifiedTableName] {
        &[]
    }

    /// 始终跳过这些表，优先级高于 `tables()`。由框架在调用 `matches()` 前自动检查。
    fn skip_tables(&self) -> &[QualifiedTableName] {
        &[]
    }

    /// 插件自身的匹配逻辑（操作类型、extensions 等）。
    fn matches(&self, ctx: &SqlRewriteContext) -> bool;

    /// 具体重写逻辑
    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()>;
}
