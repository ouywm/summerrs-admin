mod backend;
pub mod config;
pub mod error;
mod factory;
pub mod plugin;
mod provider;
mod registry;
pub mod service;

pub use config::RigConfig;
pub use plugin::SummerRigPlugin;
pub use service::{PromptOptions, PromptStream, ProviderMetadata, RigService, StreamChunk};
