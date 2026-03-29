mod snowflake;
mod tsid;

use std::sync::Arc;

use crate::{
    config::KeyGeneratorConfig,
    error::{Result, ShardingError},
};

pub use snowflake::SnowflakeKeyGenerator;
pub use tsid::TsidKeyGenerator;

pub trait KeyGenerator: Send + Sync + 'static {
    fn next_id(&self) -> i64;
    fn generator_type(&self) -> &str;
}

#[derive(Debug, Default, Clone)]
pub struct KeyGeneratorRegistry;

impl KeyGeneratorRegistry {
    pub fn build(&self, config: &KeyGeneratorConfig) -> Result<Arc<dyn KeyGenerator>> {
        match config.kind.as_str() {
            "snowflake" => Ok(Arc::new(SnowflakeKeyGenerator::from_config(config)?)),
            "tsid" => Ok(Arc::new(TsidKeyGenerator::from_config(config))),
            other => Err(ShardingError::Config(format!(
                "unsupported key generator `{other}`"
            ))),
        }
    }
}
