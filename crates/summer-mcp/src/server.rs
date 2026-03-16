use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        prompt::PromptContext,
        router::{prompt::PromptRouter, tool::ToolRouter},
    },
    model::{
        AnnotateAble, GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, RawResource,
        RawResourceTemplate, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool_handler,
};
use sea_orm::DatabaseConnection;
use summer_domain::{dict::DictDomainService, menu::MenuDomainService};

use crate::table_tools;
use crate::{
    config::McpConfig,
    error_model::{internal_error, normalize_resource_error, resource_not_found_error},
    prompts,
};

#[derive(Clone)]
struct McpServerInfo {
    server_info: Implementation,
    instructions: Option<String>,
}

impl McpServerInfo {
    fn from_config(config: &McpConfig) -> Self {
        let mut impl_info = Implementation::new(&config.server_name, &config.server_version);
        impl_info = impl_info.with_title(
            config
                .title
                .clone()
                .unwrap_or_else(|| "Summerrs Admin MCP".to_string()),
        );
        impl_info = impl_info.with_description(
            config
                .description
                .clone()
                .unwrap_or_else(|| {
                    "Summerrs Admin MCP server for schema discovery, generic table tools, SQL escape hatches, code generation, and menu/dict business operations.".to_string()
                }),
        );
        Self {
            server_info: impl_info,
            instructions: Some(
                config.instructions.clone().unwrap_or_else(|| {
                    "Call `server_capabilities` first when you need a runtime snapshot of the MCP version, transport mode, published tools/resources/prompts, generator presets, and database connectivity. Use schema resources first: read `schema://tables` or `schema://table/{table}` before calling generic table tools. Prefer `table_get`, `table_query`, `table_insert`, `table_update`, and `table_delete` over guessing database structure. Use `sql_query_readonly` only when the read shape is too complex for `table_query`. Use `sql_exec` for targeted DDL/DML statements, not for reads. Use `generate_entity_from_table` to sync SeaORM entities from live tables, then `upgrade_entity_enums_from_table` when comment/native enum semantics should become real `DeriveActiveEnum` types in the entity model. Use `generate_admin_module_from_table` to scaffold backend CRUD modules, and `generate_frontend_bundle_from_table` to generate frontend api/type/page bundles in one shot. For real Art Design Pro projects, set `target_preset=art_design_pro` so files land under `src/api`, `src/types/api`, and `src/views/system`. Use `menu_tool` and `dict_tool` for business-level menu and dictionary operations instead of raw SQL, and prefer their plan/export/apply actions when AI needs to stage structured configuration safely. When the client supports prompts, use the published workflow prompts to follow the recommended discovery, generation, and rollout sequence.".to_string()
                }),
            ),
        }
    }
}

#[derive(Clone)]
pub struct AdminMcpServer {
    info: McpServerInfo,
    config: McpConfig,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
    db: DatabaseConnection,
}

impl AdminMcpServer {
    pub fn new(config: &McpConfig, db: DatabaseConnection) -> Self {
        Self {
            info: McpServerInfo::from_config(config),
            config: config.clone(),
            tool_router: Self::tool_router(),
            prompt_router: prompts::build_prompt_router(),
            db,
        }
    }

    pub(crate) fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    pub(crate) fn config(&self) -> &McpConfig {
        &self.config
    }

    pub(crate) fn default_database_url(&self) -> Option<&str> {
        self.config.default_database_url.as_deref()
    }

    pub(crate) fn menu_domain(&self) -> MenuDomainService {
        MenuDomainService::new(self.db.clone())
    }

    pub(crate) fn dict_domain(&self) -> DictDomainService {
        DictDomainService::new(self.db.clone())
    }
}

pub(crate) fn schema_tables_resource() -> rmcp::model::Resource {
    RawResource::new("schema://tables", "tables")
        .with_title("Database Tables")
        .with_description("Runtime-discovered public tables exposed by summer-mcp")
        .with_mime_type("application/json")
        .no_annotation()
}

pub(crate) fn schema_table_resource(table: &str) -> rmcp::model::Resource {
    RawResource::new(
        format!("schema://table/{table}"),
        format!("table_schema_{table}"),
    )
    .with_title(format!("Table Schema: {table}"))
    .with_description("Read live schema metadata for a database table")
    .with_mime_type("application/json")
    .no_annotation()
}

#[tool_handler]
impl ServerHandler for AdminMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_prompts()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(self.info.server_info.clone());
        if let Some(instructions) = &self.info.instructions {
            info = info.with_instructions(instructions);
        }
        info
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![schema_tables_resource()],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri;
        match uri.as_str() {
            "schema://tables" => {
                let tables = table_tools::list_tables(self.db())
                    .await
                    .map_err(|error| normalize_resource_error("schema://tables", error))?;
                let body = serde_json::to_string_pretty(&serde_json::json!({
                    "schema": "public",
                    "tables": tables,
                }))
                .map_err(|error| {
                    internal_error(
                        "serialization_failed",
                        "Serialization failed",
                        Some("Check that the resource payload can be encoded as JSON."),
                        Some(error.to_string()),
                        Some(serde_json::json!({ "resource": "schema://tables" })),
                    )
                })?;
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(body, uri).with_mime_type("application/json"),
                ]))
            }
            _ if uri.starts_with("schema://table/") => {
                let table = uri.trim_start_matches("schema://table/");
                let schema = table_tools::describe_table(self.db(), table)
                    .await
                    .map_err(|error| normalize_resource_error(&uri, error))?;
                let body = serde_json::to_string_pretty(&schema).map_err(|error| {
                    internal_error(
                        "serialization_failed",
                        "Serialization failed",
                        Some("Check that the resource payload can be encoded as JSON."),
                        Some(error.to_string()),
                        Some(serde_json::json!({ "resource": uri })),
                    )
                })?;
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(body, uri).with_mime_type("application/json"),
                ]))
            }
            _ => Err(resource_not_found_error(
                "resource_not_found",
                "Resource not found",
                Some(
                    "Call list_resources or list_resource_templates first to confirm the published resource paths.",
                ),
                Some(format!("resource `{uri}` not found")),
                Some(serde_json::json!({ "resource": uri })),
            )),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                RawResourceTemplate::new("schema://table/{table}", "table_schema")
                    .with_title("Table Schema")
                    .with_description("Read live schema metadata for a database table")
                    .with_mime_type("application/json")
                    .no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: self.prompt_router.list_all(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        self.prompt_router
            .get_prompt(PromptContext::new(
                self,
                request.name,
                request.arguments,
                ctx,
            ))
            .await
    }
}
