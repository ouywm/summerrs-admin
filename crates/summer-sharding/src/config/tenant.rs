use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenantIsolationLevel {
    #[default]
    SharedRow,
    SeparateTable,
    SeparateSchema,
    SeparateDatabase,
}

impl TenantIsolationLevel {
    pub const fn code(self) -> i16 {
        match self {
            Self::SharedRow => 1,
            Self::SeparateTable => 2,
            Self::SeparateSchema => 3,
            Self::SeparateDatabase => 4,
        }
    }

    pub const fn from_code(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::SharedRow),
            2 => Some(Self::SeparateTable),
            3 => Some(Self::SeparateSchema),
            4 => Some(Self::SeparateDatabase),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenantIdSource {
    #[default]
    RequestExtension,
    Header,
    JwtClaim,
    QueryParam,
    #[serde(alias = "request_context")]
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenantRowLevelStrategy {
    #[default]
    SqlRewrite,
    Rls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantRowLevelConfig {
    #[serde(default = "default_tenant_column")]
    pub column_name: String,
    #[serde(default)]
    pub strategy: TenantRowLevelStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tenant_id_source: TenantIdSource,
    #[serde(default = "default_tenant_field")]
    pub tenant_id_field: String,
    #[serde(default)]
    pub default_isolation: TenantIsolationLevel,
    #[serde(default)]
    pub shared_tables: Vec<String>,
    #[serde(default)]
    pub row_level: TenantRowLevelConfig,
}

impl Default for TenantRowLevelConfig {
    fn default() -> Self {
        Self {
            column_name: default_tenant_column(),
            strategy: TenantRowLevelStrategy::SqlRewrite,
        }
    }
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tenant_id_source: TenantIdSource::RequestExtension,
            tenant_id_field: default_tenant_field(),
            default_isolation: TenantIsolationLevel::SharedRow,
            shared_tables: Vec::new(),
            row_level: TenantRowLevelConfig::default(),
        }
    }
}

fn default_tenant_field() -> String {
    "tenant_id".to_string()
}

fn default_tenant_column() -> String {
    "tenant_id".to_string()
}
