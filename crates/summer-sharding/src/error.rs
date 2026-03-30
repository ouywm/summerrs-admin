use sea_orm::DbErr;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ShardingError>;

#[derive(Debug, Error)]
pub enum ShardingError {
    #[error("invalid sharding config: {0}")]
    Config(String),
    #[error("sharding parse error: {0}")]
    Parse(String),
    #[error("sharding route error: {0}")]
    Route(String),
    #[error("sharding rewrite error: {0}")]
    Rewrite(String),
    #[error("unsupported sharding operation: {0}")]
    Unsupported(String),
    #[error("datasource not found: {0}")]
    DataSourceNotFound(String),
    #[error("table rule not found: {0}")]
    TableRuleNotFound(String),
    #[error("missing sharding value for table `{table}` column `{column}`")]
    MissingShardingValue { table: String, column: String },
    #[error("plugin `{plugin}` rewrite error: {message}")]
    Plugin { plugin: String, message: String },
    #[error("database error: {0}")]
    Db(#[from] DbErr),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<sqlparser::parser::ParserError> for ShardingError {
    fn from(value: sqlparser::parser::ParserError) -> Self {
        Self::Parse(value.to_string())
    }
}

impl From<ShardingError> for DbErr {
    fn from(value: ShardingError) -> Self {
        DbErr::Custom(value.to_string())
    }
}
