use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigError {
    Config(String),
    ProviderNotFound(String),
    DefaultModelMissing(String),
    BackendInit(String),
    PromptFailed(String),
    StreamFailed(String),
}

impl Display for RigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RigError::Config(message) => write!(f, "config error: {message}"),
            RigError::ProviderNotFound(name) => write!(f, "provider not found: {name}"),
            RigError::DefaultModelMissing(name) => {
                write!(f, "provider '{name}' missing default model")
            }
            RigError::BackendInit(message) => write!(f, "backend init failed: {message}"),
            RigError::PromptFailed(message) => write!(f, "prompt failed: {message}"),
            RigError::StreamFailed(message) => write!(f, "stream failed: {message}"),
        }
    }
}

impl std::error::Error for RigError {}
