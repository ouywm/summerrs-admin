use crate::{
    config::ShardingConfig,
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub struct SchemaRouter {
    config: ShardingConfig,
}

impl SchemaRouter {
    pub fn new(config: &ShardingConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub fn route(&self, schema: Option<&str>) -> Result<String> {
        if let Some(schema) = schema
            && let Some(datasource) = self.config.schema_primary_datasource(schema)
        {
            return Ok(datasource.to_string());
        }

        self.config
            .default_datasource_name()
            .map(str::to_string)
            .ok_or_else(|| ShardingError::Route("default datasource is not configured".to_string()))
    }
}
