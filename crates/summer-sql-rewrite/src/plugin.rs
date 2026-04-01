use crate::{context::SqlRewriteContext, error::Result};

pub trait SqlRewritePlugin: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn order(&self) -> i32 {
        100
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool;

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()>;
}
