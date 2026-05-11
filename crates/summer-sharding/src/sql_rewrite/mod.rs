//! SQL 改写插件框架——为 sharding 提供 plugin pipeline 能力。
//!
//! 这套子模块原本是独立 crate `summer-sql-rewrite`，已合并进来。提供：
//!
//! - `SqlRewritePlugin` trait + `PluginRegistry`：可注册的 AST 改写插件
//! - `SqlRewriteContext`：插件运行时上下文（语句、操作类型、表名、扩展容器、注释）
//! - `Extensions`：类型安全的请求级容器，插件之间共享数据
//! - `helpers`：常用 AST 操作辅助函数（注入 WHERE、改表名、构造表达式等）
//! - `pipeline::rewrite_statement`：跑一遍 plugin pipeline 的便捷函数
//! - `QualifiedTableName`：schema/table 分离的表名表示，含大小写不敏感匹配
//! - `SqlRewriteConfigurator`：扩展 `AppBuilder` 提供 `.sql_rewrite_configure(...)` 入口
//! - `ProbePlugin`：探针插件，不改 SQL 只统计命中次数，用于集成测试

pub mod builtin;
pub mod configurator;
pub mod context;
pub mod error;
pub mod extensions;
pub mod helpers;
pub mod pipeline;
pub mod plugin;
pub mod registry;
pub mod table;

pub use builtin::ProbePlugin;
pub use configurator::SqlRewriteConfigurator;
pub use context::{SqlOperation, SqlRewriteContext};
pub use error::{Result, SqlRewriteError};
pub use extensions::Extensions;
pub use plugin::SqlRewritePlugin;
pub use registry::PluginRegistry;
pub use table::QualifiedTableName;
