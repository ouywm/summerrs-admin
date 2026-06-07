use rmcp::schemars;
use sea_orm::{DatabaseConnection, DbBackend};
use serde::{Deserialize, Serialize};

use crate::{
    prompts, server::AdminMcpServer, table_tools::schema::list_tables,
    tools::frontend_target::FrontendTargetPreset,
};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServerHealthStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ServerCapabilitiesResult {
    pub health: ServerHealthSummary,
    pub server: ServerIdentitySummary,
    pub runtime: ServerRuntimeSummary,
    pub capabilities: ServerCapabilityCatalog,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ServerHealthSummary {
    pub status: ServerHealthStatus,
    pub database: DatabaseHealthSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct DatabaseHealthSummary {
    pub backend: String,
    pub connected: bool,
    pub public_table_count: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ServerIdentitySummary {
    pub name: String,
    pub version: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ServerRuntimeSummary {
    pub transport: String,
    pub http_mode: String,
    pub binding: String,
    pub port: u16,
    pub path: String,
    pub stateful_mode: bool,
    pub json_response: bool,
    pub session_channel_capacity: usize,
    pub session_keep_alive_seconds: Option<u64>,
    pub default_database_url_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ServerCapabilityCatalog {
    pub tools: Vec<String>,
    pub prompts: Vec<String>,
    pub resources: Vec<ResourceCapabilitySummary>,
    pub resource_templates: Vec<ResourceTemplateCapabilitySummary>,
    pub generators: GeneratorCapabilitySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ResourceCapabilitySummary {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ResourceTemplateCapabilitySummary {
    pub uri_template: String,
    pub name: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct GeneratorCapabilitySummary {
    pub backend_generators: Vec<String>,
    pub frontend_generators: Vec<String>,
    pub frontend_target_presets: Vec<String>,
    pub supports_temp_output_dir: bool,
    pub returns_menu_dict_drafts: bool,
}

pub(crate) async fn inspect_database_health(db: &DatabaseConnection) -> DatabaseHealthSummary {
    match list_tables(db).await {
        Ok(tables) => DatabaseHealthSummary {
            backend: database_backend_name(db),
            connected: true,
            public_table_count: Some(tables.len()),
            error: None,
        },
        Err(error) => DatabaseHealthSummary {
            backend: database_backend_name(db),
            connected: false,
            public_table_count: None,
            error: Some(error.message.to_string()),
        },
    }
}

pub(crate) fn tool_catalog() -> Vec<String> {
    let mut names = AdminMcpServer::tool_router()
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

pub(crate) fn prompt_catalog() -> Vec<String> {
    let mut names = prompts::build_prompt_router()
        .list_all()
        .into_iter()
        .map(|prompt| prompt.name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

pub(crate) fn resource_catalog() -> Vec<ResourceCapabilitySummary> {
    vec![ResourceCapabilitySummary {
        uri: "schema://tables".to_string(),
        name: "tables".to_string(),
        title: Some("Database Tables".to_string()),
    }]
}

pub(crate) fn resource_template_catalog() -> Vec<ResourceTemplateCapabilitySummary> {
    vec![ResourceTemplateCapabilitySummary {
        uri_template: "schema://table/{table}".to_string(),
        name: "table_schema".to_string(),
        title: Some("Table Schema".to_string()),
    }]
}

pub(crate) fn generator_capability_catalog() -> GeneratorCapabilitySummary {
    GeneratorCapabilitySummary {
        backend_generators: vec![
            "generate_entity_from_table".to_string(),
            "upgrade_entity_enums_from_table".to_string(),
            "generate_admin_module_from_table".to_string(),
        ],
        frontend_generators: vec![
            "generate_frontend_api_from_table".to_string(),
            "generate_frontend_page_from_table".to_string(),
            "generate_frontend_bundle_from_table".to_string(),
        ],
        frontend_target_presets: vec![
            FrontendTargetPreset::SummerMcp.as_str().to_string(),
            FrontendTargetPreset::ArtDesignPro.as_str().to_string(),
        ],
        supports_temp_output_dir: true,
        returns_menu_dict_drafts: true,
    }
}

fn database_backend_name(db: &DatabaseConnection) -> String {
    match db.get_database_backend() {
        DbBackend::MySql => "mysql",
        DbBackend::Postgres => "postgres",
        DbBackend::Sqlite => "sqlite",
        _ => "unknown",
    }
    .to_string()
}
