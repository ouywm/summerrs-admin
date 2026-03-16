pub mod config;
mod error_model;
mod output_contract;
pub mod plugin;
mod prompts;
pub mod runtime;
pub mod server;
mod table_tools;
mod tools;

pub use plugin::McpPlugin;
pub use runtime::{McpRuntimeError, run_server, run_server_with_shutdown};
pub use server::AdminMcpServer;
