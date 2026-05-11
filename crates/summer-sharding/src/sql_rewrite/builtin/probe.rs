//! 探针插件——不改写 SQL，只统计命中次数 + 写注释。
//!
//! 用于验证插件管道是否真的被执行，适合集成测试和调试。
//!
//! ```rust,ignore
//! use summer_sql_rewrite::{ProbePlugin, PluginRegistry};
//!
//! let probe = ProbePlugin::new();
//! let mut registry = PluginRegistry::new();
//! registry.register(probe.clone());
//!
//! // ... 执行 SQL ...
//!
//! assert!(probe.hit_count() > 0, "plugin was never called");
//! ```

use crate::sql_rewrite::{Result, SqlRewriteContext, SqlRewritePlugin};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// 探针插件，不修改 SQL，只统计命中次数并写注释。
///
/// 可以 clone，所有 clone 共享同一个计数器。
#[derive(Clone, Default)]
pub struct ProbePlugin {
    counter: Arc<AtomicUsize>,
}

impl ProbePlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回插件被调用的次数。
    pub fn hit_count(&self) -> usize {
        self.counter.load(Ordering::Relaxed)
    }

    /// 重置计数器。
    pub fn reset(&self) {
        self.counter.store(0, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for ProbePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbePlugin")
            .field("hit_count", &self.hit_count())
            .finish()
    }
}

impl SqlRewritePlugin for ProbePlugin {
    fn name(&self) -> &str {
        "probe"
    }

    fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
        true
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let count = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        ctx.append_comment(&format!("probe={count}"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, Statement};

    use crate::sql_rewrite::{Extensions, PluginRegistry, pipeline::rewrite_statement};

    use super::*;

    #[test]
    fn probe_counts_hits_and_appends_comment() {
        let probe = ProbePlugin::new();
        let mut registry = PluginRegistry::new();
        registry.register(probe.clone());

        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT 1");
        let result = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");

        assert_eq!(probe.hit_count(), 1);
        assert!(result.sql.contains("probe=1"), "sql: {}", result.sql);
    }

    #[test]
    fn probe_clone_shares_counter() {
        let probe = ProbePlugin::new();
        let probe2 = probe.clone();
        let mut registry = PluginRegistry::new();
        registry.register(probe.clone());

        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT 1");
        rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");

        assert_eq!(probe2.hit_count(), 1);
    }

    #[test]
    fn probe_reset_clears_counter() {
        let probe = ProbePlugin::new();
        let mut registry = PluginRegistry::new();
        registry.register(probe.clone());

        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT 1");
        rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");
        assert_eq!(probe.hit_count(), 1);

        probe.reset();
        assert_eq!(probe.hit_count(), 0);
    }
}
