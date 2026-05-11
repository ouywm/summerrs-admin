#![doc = include_str!("../README.md")]

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

pub use configurator::SqlRewriteConfigurator;
pub use context::{SqlOperation, SqlRewriteContext};
pub use error::{Result, SqlRewriteError};
pub use extensions::Extensions;
pub use plugin::SqlRewritePlugin;
pub use registry::PluginRegistry;
pub use table::QualifiedTableName;
