use rmcp::{ErrorData as McpError, Json, handler::server::wrapper::Parameters, tool, tool_router};
use sea_orm::{
    AccessMode, ConnectionTrait, DbBackend, FromQueryResult, JsonValue, SelectModel, SelectorRaw,
    Statement, TransactionError, TransactionTrait,
};

use crate::{
    error_model::normalize_tool_error,
    output_contract::ToolExecutionMode,
    server::AdminMcpServer,
    table_tools::{
        artifacts::{
            build_admin_generator_artifacts, build_entity_generator_artifacts,
            build_frontend_api_artifacts, build_frontend_bundle_artifacts,
            build_frontend_page_artifacts, export_dict_bundle_artifacts,
            export_menu_config_artifacts,
        },
        capabilities::{
            ServerCapabilitiesResult, ServerCapabilityCatalog, ServerHealthStatus,
            ServerHealthSummary, ServerIdentitySummary, ServerRuntimeSummary,
            generator_capability_catalog, inspect_database_health, prompt_catalog,
            resource_catalog, resource_template_catalog, tool_catalog,
        },
        error_bridge::{
            api_error_to_mcp, empty_dict_data_query, empty_dict_type_query, operator_name,
            sql_tool_db_error,
        },
        query_builder::{
            build_filters_clause, build_insert_assignments, build_key_clause, build_order_clause,
            build_update_assignments,
        },
        schema::{
            TableSchema, db_error, describe_table_for_crud_in_schema, describe_table_in_schema,
            ensure_valid_identifier, list_tables_in_schema, normalize_schema, quote_identifier,
            readable_select_list,
        },
        sql_scanner::{convert_sql_params, normalize_exec_sql, normalize_readonly_sql},
        tool_args::*,
        tool_results::*,
    },
    tools::{
        admin_module_generator::{AdminModuleGenerator, GenerateAdminModuleRequest},
        entity_enum_upgrader::EntityEnumUpgrader,
        entity_generator::{EntityGenerator, GenerateEntityRequest},
        frontend_api_generator::{FrontendApiGenerator, GenerateFrontendApiRequest},
        frontend_bundle_generator::{FrontendBundleGenerator, GenerateFrontendBundleRequest},
        frontend_page_generator::{FrontendPageGenerator, GenerateFrontendPageRequest},
        generation_context::CrudFieldSelection,
        support::workspace_root,
        validation::validate_frontend_target_output,
    },
};

const DEFAULT_LIST_LIMIT: u64 = 20;
const MAX_LIST_LIMIT: u64 = 100;
const DEFAULT_SQL_QUERY_LIMIT: u64 = 200;
const MAX_SQL_QUERY_LIMIT: u64 = 1_000;
const READONLY_SQL_SUBQUERY_ALIAS: &str = "__summer_mcp_readonly";

fn normalize_tool_result<T>(
    tool: &'static str,
    result: Result<T, McpError>,
) -> Result<T, McpError> {
    result.map_err(|error| normalize_tool_error(tool, error))
}

macro_rules! tool_result {
    ($tool:literal, $body:block) => {{
        normalize_tool_result($tool, (async $body).await)
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListWindow {
    limit: u64,
    offset: u64,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    total: i64,
}

impl ListWindow {
    fn from_args(limit: Option<u64>, offset: Option<u64>) -> Self {
        Self {
            limit: limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT),
            offset: offset.unwrap_or_default(),
        }
    }
}

fn validate_crud_field_selection(field_selection: &CrudFieldSelection) -> Result<(), McpError> {
    for (label, fields) in [
        ("query_fields", field_selection.query_fields.as_deref()),
        ("create_fields", field_selection.create_fields.as_deref()),
        ("update_fields", field_selection.update_fields.as_deref()),
        ("list_fields", field_selection.list_fields.as_deref()),
        ("detail_fields", field_selection.detail_fields.as_deref()),
    ] {
        if let Some(fields) = fields {
            for field in fields {
                ensure_valid_identifier(field, label)?;
            }
        }
    }
    Ok(())
}

fn build_crud_field_selection(
    query_fields: Option<Vec<String>>,
    create_fields: Option<Vec<String>>,
    update_fields: Option<Vec<String>>,
    list_fields: Option<Vec<String>>,
    detail_fields: Option<Vec<String>>,
) -> CrudFieldSelection {
    CrudFieldSelection {
        query_fields,
        create_fields,
        update_fields,
        list_fields,
        detail_fields,
    }
}

fn normalize_explicit_schema(schema: Option<&str>) -> Result<Option<String>, McpError> {
    schema
        .map(|value| normalize_schema(Some(value)))
        .transpose()
}

fn search_path_statement(schema: &str) -> Statement {
    Statement::from_string(
        DbBackend::Postgres,
        format!(
            "SET LOCAL search_path TO {}, pg_catalog",
            quote_identifier(schema)
        ),
    )
}

#[tool_router(router = tool_router, vis = "pub(crate)")]
impl AdminMcpServer {
    #[tool(
        description = "Inspect MCP health, version, runtime config summary, published tools/resources/prompts, and generator capability presets"
    )]
    async fn server_capabilities(&self) -> Result<Json<ServerCapabilitiesResult>, McpError> {
        let config = self.config();
        let database = inspect_database_health(self.db()).await;
        let health = ServerHealthSummary {
            status: if database.connected {
                ServerHealthStatus::Ok
            } else {
                ServerHealthStatus::Degraded
            },
            database,
        };

        Ok(Json(ServerCapabilitiesResult {
            health,
            server: ServerIdentitySummary {
                name: config.server_name.clone(),
                version: config.server_version.clone(),
                title: config.title.clone(),
                description: config.description.clone(),
            },
            runtime: ServerRuntimeSummary {
                transport: config.transport.to_string(),
                http_mode: config.http_mode.to_string(),
                binding: config.binding.to_string(),
                port: config.port,
                path: config.path.clone(),
                stateful_mode: config.stateful_mode,
                json_response: config.json_response,
                allowed_hosts: config.allowed_hosts.clone(),
                allowed_origins: config.allowed_origins.clone(),
                session_channel_capacity: config.session_channel_capacity,
                session_keep_alive_seconds: config.session_keep_alive,
                default_database_url_available: config.default_database_url.is_some(),
            },
            capabilities: ServerCapabilityCatalog {
                tools: tool_catalog(),
                prompts: prompt_catalog(),
                resources: resource_catalog(),
                resource_templates: resource_template_catalog(),
                generators: generator_capability_catalog(),
            },
        }))
    }

    #[tool(description = "List runtime-discovered database tables exposed by this MCP server")]
    async fn schema_list_tables(
        &self,
        Parameters(args): Parameters<ListTablesArgs>,
    ) -> Result<Json<ListTablesResult>, McpError> {
        tool_result!("schema_list_tables", {
            let schema = normalize_schema(args.schema.as_deref())?;
            let tables = list_tables_in_schema(self.db(), &schema).await?;
            Ok(Json(ListTablesResult { schema, tables }))
        })
    }

    #[tool(
        description = "Describe a database table at runtime, including primary keys and readable/writable columns"
    )]
    async fn schema_describe_table(
        &self,
        Parameters(args): Parameters<DescribeTableArgs>,
    ) -> Result<Json<TableSchema>, McpError> {
        tool_result!("schema_describe_table", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema = describe_table_in_schema(self.db(), &schema_name, &args.table).await?;
            Ok(Json(schema))
        })
    }

    #[tool(
        description = "Generate or regenerate one SeaORM entity file from a live database table via sea-orm-cli and sync crates/model/src/entity/mod.rs"
    )]
    async fn generate_entity_from_table(
        &self,
        Parameters(args): Parameters<GenerateEntityFromTableArgs>,
    ) -> Result<Json<GenerateEntityFromTableResult>, McpError> {
        tool_result!("generate_entity_from_table", {
            ensure_valid_identifier(&args.table, "table")?;
            let table = args.table.clone();
            let overwrite = args.overwrite.unwrap_or(false);
            let output_dir = args.output_dir.clone();
            let database_url = args.database_url.clone();
            let database_schema = args.database_schema.clone();
            let cli_bin = args.cli_bin.clone();
            let enum_name_overrides = args.enum_name_overrides.clone();
            let variant_name_overrides = args.variant_name_overrides.clone();
            for field in enum_name_overrides.keys() {
                ensure_valid_identifier(field, "enum_name_overrides field")?;
            }
            for field in variant_name_overrides.keys() {
                ensure_valid_identifier(field, "variant_name_overrides field")?;
            }
            let schema_name = normalize_schema(database_schema.as_deref())?;
            let schema = describe_table_for_crud_in_schema(self.db(), &schema_name, &table).await?;

            let generator =
                EntityGenerator::new(self.default_database_url().map(ToOwned::to_owned))?;
            let result = generator
                .generate(GenerateEntityRequest {
                    table,
                    overwrite,
                    output_dir: output_dir.clone(),
                    database_url,
                    database_schema,
                    cli_bin,
                    schema: Some(schema),
                    enum_name_overrides,
                    variant_name_overrides,
                })
                .await?;
            let artifacts = build_entity_generator_artifacts(
                output_dir.as_deref(),
                &result.entity_file,
                &result.mod_file,
            );

            Ok(Json(GenerateEntityFromTableResult {
                table: result.table,
                entity_file: result.entity_file.display().to_string(),
                mod_file: result.mod_file.display().to_string(),
                overwritten: result.overwritten,
                database_schema: result.database_schema,
                cli_bin: result.cli_bin,
                enum_upgrade_changed: result.enum_upgrade_changed,
                enum_upgrade_fields: result.enum_upgrade_fields,
                artifacts,
                validation: result.validation,
            }))
        })
    }

    #[tool(
        description = "Plan or apply SeaORM entity enum upgrades by turning semantic enum drafts into DeriveActiveEnum definitions and typed Model fields"
    )]
    async fn upgrade_entity_enums_from_table(
        &self,
        Parameters(args): Parameters<UpgradeEntityEnumsFromTableArgs>,
    ) -> Result<Json<UpgradeEntityEnumsFromTableResult>, McpError> {
        tool_result!("upgrade_entity_enums_from_table", {
            let (
                mode,
                schema,
                table,
                route_base,
                output_dir,
                fields,
                enum_name_overrides,
                variant_name_overrides,
            ) = match args {
                UpgradeEntityEnumsFromTableArgs::PlanUpgrade {
                    schema,
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                } => (
                    ToolExecutionMode::Plan,
                    schema,
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                ),
                UpgradeEntityEnumsFromTableArgs::ApplyUpgrade {
                    schema,
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                } => (
                    ToolExecutionMode::Apply,
                    schema,
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                ),
            };

            ensure_valid_identifier(&table, "table")?;
            if let Some(route_base) = &route_base {
                ensure_valid_identifier(route_base, "route_base")?;
            }
            if let Some(fields) = &fields {
                for field in fields {
                    ensure_valid_identifier(field, "fields")?;
                }
            }
            for field in enum_name_overrides.keys() {
                ensure_valid_identifier(field, "enum_name_overrides field")?;
            }
            for field in variant_name_overrides.keys() {
                ensure_valid_identifier(field, "variant_name_overrides field")?;
            }

            let schema_name = normalize_schema(schema.as_deref())?;
            let schema = describe_table_for_crud_in_schema(self.db(), &schema_name, &table).await?;
            let upgrader = EntityEnumUpgrader::new()?;
            let request = crate::tools::entity_enum_upgrader::EntityEnumUpgradeRequest {
                schema,
                route_base,
                output_dir: output_dir.clone(),
                fields,
                enum_name_overrides,
                variant_name_overrides,
            };

            let response = match mode {
                ToolExecutionMode::Plan => {
                    let preview = upgrader.plan(request).await?;
                    UpgradeEntityEnumsFromTableResult {
                        mode,
                        table: preview.table,
                        route_base: preview.route_base,
                        entity_file: preview.entity_file.display().to_string(),
                        changed: preview.changed,
                        plan: preview.plan,
                        rendered_source: preview.rendered_source,
                        artifacts: preview.artifacts,
                        validation: None,
                    }
                }
                ToolExecutionMode::Apply => {
                    let result = upgrader.apply(request).await?;
                    UpgradeEntityEnumsFromTableResult {
                        mode,
                        table: result.preview.table,
                        route_base: result.preview.route_base,
                        entity_file: result.preview.entity_file.display().to_string(),
                        changed: result.preview.changed,
                        plan: result.preview.plan,
                        rendered_source: result.preview.rendered_source,
                        artifacts: result.preview.artifacts,
                        validation: Some(result.validation),
                    }
                }
                _ => unreachable!("entity enum upgrader only supports plan/apply"),
            };

            Ok(Json(response))
        })
    }

    #[tool(
        description = "Generate a compile-ready admin CRUD skeleton for one single-primary-key table, including router/service/dto/vo modules. Pass output_dir to write into a temp directory instead of the workspace."
    )]
    async fn generate_admin_module_from_table(
        &self,
        Parameters(args): Parameters<GenerateAdminModuleFromTableArgs>,
    ) -> Result<Json<GenerateAdminModuleFromTableResult>, McpError> {
        tool_result!("generate_admin_module_from_table", {
            ensure_valid_identifier(&args.table, "table")?;
            if let Some(route_base) = &args.route_base {
                ensure_valid_identifier(route_base, "route_base")?;
            }
            let field_selection = build_crud_field_selection(
                args.query_fields.clone(),
                args.create_fields.clone(),
                args.update_fields.clone(),
                args.list_fields.clone(),
                args.detail_fields.clone(),
            );
            validate_crud_field_selection(&field_selection)?;

            let route_base = args.route_base.clone();
            let output_dir = args.output_dir.clone();
            let overwrite = args.overwrite.unwrap_or(false);
            let workspace_root = workspace_root()?;
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema = describe_table_in_schema(self.db(), &schema_name, &args.table).await?;
            let generator = AdminModuleGenerator::new()?;
            let result = generator
                .generate(GenerateAdminModuleRequest {
                    schema,
                    overwrite,
                    route_base,
                    output_dir: output_dir.clone(),
                    field_selection,
                })
                .await?;
            let artifacts = build_admin_generator_artifacts(
                &workspace_root,
                output_dir.as_deref(),
                &result.router_file,
                &result.service_file,
                &result.dto_file,
                &result.vo_file,
                &result.updated_mod_files,
            );

            Ok(Json(GenerateAdminModuleFromTableResult {
                table: result.table,
                route_base: result.route_base,
                router_file: result.router_file.display().to_string(),
                service_file: result.service_file.display().to_string(),
                dto_file: result.dto_file.display().to_string(),
                vo_file: result.vo_file.display().to_string(),
                updated_mod_files: result
                    .updated_mod_files
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                artifacts,
                validation: result.validation,
            }))
        })
    }

    #[tool(
        description = "Generate frontend API wrappers and global TypeScript declarations for one table. Default target_preset=summer_mcp writes into crates/app/frontend-routes/api and api_type; art_design_pro writes into src/api and src/types/api."
    )]
    async fn generate_frontend_api_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendApiFromTableArgs>,
    ) -> Result<Json<GenerateFrontendApiFromTableResult>, McpError> {
        tool_result!("generate_frontend_api_from_table", {
            ensure_valid_identifier(&args.table, "table")?;
            if let Some(route_base) = &args.route_base {
                ensure_valid_identifier(route_base, "route_base")?;
            }
            let field_selection = build_crud_field_selection(
                args.query_fields.clone(),
                args.create_fields.clone(),
                args.update_fields.clone(),
                args.list_fields.clone(),
                args.detail_fields.clone(),
            );
            validate_crud_field_selection(&field_selection)?;

            let target_preset = args.target_preset.unwrap_or_default();
            let route_base = args.route_base.clone();
            let output_dir = args.output_dir.clone();
            let overwrite = args.overwrite.unwrap_or(false);
            let workspace_root = workspace_root()?;
            let frontend_root_dir = target_preset
                .resolve_bundle_layout(&workspace_root, output_dir.as_deref())?
                .frontend_root_dir;
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema = describe_table_in_schema(self.db(), &schema_name, &args.table).await?;
            let generator = FrontendApiGenerator::new()?;
            let result = generator
                .generate(GenerateFrontendApiRequest {
                    schema,
                    overwrite,
                    route_base,
                    output_dir,
                    target_preset,
                    field_selection,
                })
                .await?;
            let artifacts = build_frontend_api_artifacts(
                args.output_dir.as_deref(),
                &frontend_root_dir,
                &result.api_file,
                &result.api_type_file,
            );
            let validation = validate_frontend_target_output(
                target_preset,
                &frontend_root_dir,
                &[result.api_file.clone(), result.api_type_file.clone()],
            )
            .await;

            Ok(Json(GenerateFrontendApiFromTableResult {
                table: result.table,
                route_base: result.route_base,
                namespace: result.namespace,
                api_file: result.api_file.display().to_string(),
                api_type_file: result.api_type_file.display().to_string(),
                artifacts,
                validation,
            }))
        })
    }

    #[tool(
        description = "Generate frontend api/type/page in one shot for one table. By default target_preset=summer_mcp writes a self-consistent generated bundle into crates/app/frontend-routes; art_design_pro writes into src/api, src/types/api, and src/views/system. The tool auto-infers enum-backed dict bindings and returns menu/dict drafts that AI can pass to menu_tool and dict_tool for review or apply."
    )]
    async fn generate_frontend_bundle_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendBundleFromTableArgs>,
    ) -> Result<Json<GenerateFrontendBundleFromTableResult>, McpError> {
        tool_result!("generate_frontend_bundle_from_table", {
            ensure_valid_identifier(&args.table, "table")?;
            if let Some(route_base) = &args.route_base {
                ensure_valid_identifier(route_base, "route_base")?;
            }
            let field_selection = build_crud_field_selection(
                args.query_fields.clone(),
                args.create_fields.clone(),
                args.update_fields.clone(),
                args.list_fields.clone(),
                args.detail_fields.clone(),
            );
            validate_crud_field_selection(&field_selection)?;

            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema = describe_table_in_schema(self.db(), &schema_name, &args.table).await?;
            let generator = FrontendBundleGenerator::new()?;
            let output_dir = args.output_dir.clone();
            let result = generator
                .generate(GenerateFrontendBundleRequest {
                    schema,
                    overwrite: args.overwrite.unwrap_or(false),
                    route_base: args.route_base,
                    output_dir: output_dir.clone(),
                    target_preset: args.target_preset.unwrap_or_default(),
                    dict_bindings: args.dict_bindings,
                    field_hints: args.field_hints,
                    field_ui_meta: args.field_ui_meta,
                    field_selection,
                    search_fields: args.search_fields,
                    table_fields: args.table_fields,
                    form_fields: args.form_fields,
                })
                .await?;
            let artifacts = build_frontend_bundle_artifacts(
                output_dir.as_deref(),
                &result.frontend_root_dir,
                &result.api_file,
                &result.api_type_file,
                &result.types_file,
                &result.index_file,
                &result.search_file,
                &result.form_panel_file,
            );

            Ok(Json(GenerateFrontendBundleFromTableResult {
                table: result.table,
                route_base: result.route_base,
                api_namespace: result.api_namespace,
                api_import_path: result.api_import_path,
                frontend_root_dir: result.frontend_root_dir.display().to_string(),
                api_file: result.api_file.display().to_string(),
                api_type_file: result.api_type_file.display().to_string(),
                page_dir: result.page_dir.display().to_string(),
                types_file: result.types_file.display().to_string(),
                index_file: result.index_file.display().to_string(),
                search_file: result.search_file.display().to_string(),
                form_panel_file: result.form_panel_file.display().to_string(),
                required_dict_types: result.required_dict_types,
                enum_drafts: result.enum_drafts,
                dict_bundle_drafts: result.dict_bundle_drafts,
                menu_config_draft: result.menu_config_draft,
                artifacts,
                validation: result.validation,
            }))
        })
    }

    #[tool(
        description = "Generate an Art Design Pro style frontend CRUD page for one table. By default the page targets the generated frontend api/type contract for the same table; advanced overrides are only needed when adapting an existing handwritten business API contract."
    )]
    async fn generate_frontend_page_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendPageFromTableArgs>,
    ) -> Result<Json<GenerateFrontendPageFromTableResult>, McpError> {
        tool_result!("generate_frontend_page_from_table", {
            ensure_valid_identifier(&args.table, "table")?;
            if let Some(route_base) = &args.route_base {
                ensure_valid_identifier(route_base, "route_base")?;
            }
            let field_selection = build_crud_field_selection(
                args.query_fields.clone(),
                args.create_fields.clone(),
                args.update_fields.clone(),
                args.list_fields.clone(),
                args.detail_fields.clone(),
            );
            validate_crud_field_selection(&field_selection)?;

            let target_preset = args.target_preset.unwrap_or_default();
            let route_base = args.route_base.clone();
            let output_dir = args.output_dir.clone();
            let overwrite = args.overwrite.unwrap_or(false);
            let workspace_root = workspace_root()?;
            let frontend_root_dir = target_preset
                .resolve_bundle_layout(&workspace_root, output_dir.as_deref())?
                .frontend_root_dir;
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema = describe_table_in_schema(self.db(), &schema_name, &args.table).await?;
            let generator = FrontendPageGenerator::new()?;
            let result = generator
                .generate(GenerateFrontendPageRequest {
                    schema,
                    overwrite,
                    route_base,
                    output_dir,
                    target_preset,
                    api_import_path: args.api_import_path,
                    api_namespace: args.api_namespace,
                    api_list_item_type_name: args.api_list_item_type_name,
                    api_detail_type_name: args.api_detail_type_name,
                    dict_bindings: args.dict_bindings,
                    field_hints: args.field_hints,
                    field_ui_meta: args.field_ui_meta,
                    field_selection,
                    search_fields: args.search_fields,
                    table_fields: args.table_fields,
                    form_fields: args.form_fields,
                })
                .await?;
            let artifacts = build_frontend_page_artifacts(
                args.output_dir.as_deref(),
                &result.page_dir,
                &result.types_file,
                &result.index_file,
                &result.search_file,
                &result.form_panel_file,
            );
            let validation = validate_frontend_target_output(
                target_preset,
                &frontend_root_dir,
                &[
                    result.types_file.clone(),
                    result.index_file.clone(),
                    result.search_file.clone(),
                    result.form_panel_file.clone(),
                ],
            )
            .await;

            Ok(Json(GenerateFrontendPageFromTableResult {
                table: result.table,
                route_base: result.route_base,
                api_import_path: result.api_import_path,
                api_namespace: result.api_namespace,
                page_dir: result.page_dir.display().to_string(),
                types_file: result.types_file.display().to_string(),
                index_file: result.index_file.display().to_string(),
                search_file: result.search_file.display().to_string(),
                form_panel_file: result.form_panel_file.display().to_string(),
                required_dict_types: result.required_dict_types,
                artifacts,
                validation,
            }))
        })
    }

    #[tool(
        description = "Read or mutate menu business data with domain rules instead of raw SQL. Required field: `action`. Supported actions: `list_tree`, `get_user_tree`, `plan_config`, `export_config`, `apply_config`, `create_menu`, `create_button`, `update_menu`, `update_button`, `delete_node`."
    )]
    async fn menu_tool(
        &self,
        Parameters(args): Parameters<MenuToolArgs>,
    ) -> Result<Json<MenuToolResponse>, McpError> {
        tool_result!("menu_tool", {
            let domain = self.menu_domain();
            let (mode, result) = match args {
                MenuToolArgs::ListTree => (
                    ToolExecutionMode::Read,
                    MenuToolResult::Tree {
                        items: domain.list_menus().await.map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::GetUserTree { user_id } => (
                    ToolExecutionMode::Read,
                    MenuToolResult::Tree {
                        items: domain
                            .get_menu_tree_for_user_id(user_id)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::PlanConfig { config } => (
                    ToolExecutionMode::Plan,
                    MenuToolResult::ConfigSync {
                        sync: domain
                            .plan_menu_config(&config)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::ExportConfig { config, output_dir } => {
                    let sync = domain
                        .plan_menu_config(&config)
                        .await
                        .map_err(api_error_to_mcp)?;
                    let export = export_menu_config_artifacts(&config, &sync, &output_dir).await?;
                    (
                        ToolExecutionMode::Export,
                        MenuToolResult::ConfigExport { export, sync },
                    )
                }
                MenuToolArgs::ApplyConfig { config } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::ConfigSync {
                        sync: domain
                            .apply_menu_config(config)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::CreateMenu { data } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::Menu {
                        item: domain.create_menu(data).await.map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::CreateButton { data } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::Menu {
                        item: domain.create_button(data).await.map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::UpdateMenu { id, data } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::Menu {
                        item: domain
                            .update_menu(id, data)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::UpdateButton { id, data } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::Menu {
                        item: domain
                            .update_button(id, data)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                MenuToolArgs::DeleteNode { id } => (
                    ToolExecutionMode::Apply,
                    MenuToolResult::Deleted {
                        id: domain.delete_menu(id).await.map_err(api_error_to_mcp)?,
                    },
                ),
            };
            Ok(Json(MenuToolResponse { mode, result }))
        })
    }

    #[tool(
        description = "Read or mutate dictionary business data with domain rules instead of raw SQL. Required field: `action`. Supported actions: `list_types`, `list_data`, `get_by_type`, `get_all_enabled`, `plan_bundle`, `export_bundle`, `apply_bundle`, `create_type`, `update_type`, `delete_type`, `create_data`, `update_data`, `delete_data`."
    )]
    async fn dict_tool(
        &self,
        Parameters(args): Parameters<DictToolArgs>,
    ) -> Result<Json<DictToolResponse>, McpError> {
        tool_result!("dict_tool", {
            let domain = self.dict_domain();
            let (mode, result) = match args {
                DictToolArgs::ListTypes { query } => (
                    ToolExecutionMode::Read,
                    DictToolResult::TypeList {
                        items: domain
                            .list_dict_types(query.unwrap_or_else(empty_dict_type_query))
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::ListData { query } => (
                    ToolExecutionMode::Read,
                    DictToolResult::DataList {
                        items: domain
                            .list_dict_data(query.unwrap_or_else(empty_dict_data_query))
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::GetByType { dict_type } => (
                    ToolExecutionMode::Read,
                    DictToolResult::SimpleDataList {
                        items: domain
                            .get_dict_data_by_type(&dict_type)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::GetAllEnabled => (
                    ToolExecutionMode::Read,
                    DictToolResult::AllData {
                        data: domain.get_all_dict_data().await.map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::PlanBundle { bundle } => (
                    ToolExecutionMode::Plan,
                    DictToolResult::BundleSync {
                        sync: domain
                            .plan_dict_bundle(&bundle)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::ExportBundle { bundle, output_dir } => {
                    let sync = domain
                        .plan_dict_bundle(&bundle)
                        .await
                        .map_err(api_error_to_mcp)?;
                    let export = export_dict_bundle_artifacts(&bundle, &sync, &output_dir).await?;
                    (
                        ToolExecutionMode::Export,
                        DictToolResult::BundleExport { export, sync },
                    )
                }
                DictToolArgs::ApplyBundle { operator, bundle } => {
                    let operator = operator_name(operator);
                    (
                        ToolExecutionMode::Apply,
                        DictToolResult::BundleSync {
                            sync: domain
                                .apply_dict_bundle(bundle, &operator)
                                .await
                                .map_err(api_error_to_mcp)?,
                        },
                    )
                }
                DictToolArgs::CreateType { operator, data } => {
                    let operator = operator_name(operator);
                    (
                        ToolExecutionMode::Apply,
                        DictToolResult::Type {
                            item: domain
                                .create_dict_type(data, &operator)
                                .await
                                .map_err(api_error_to_mcp)?,
                        },
                    )
                }
                DictToolArgs::UpdateType { id, operator, data } => {
                    let operator = operator_name(operator);
                    (
                        ToolExecutionMode::Apply,
                        DictToolResult::Type {
                            item: domain
                                .update_dict_type(id, data, &operator)
                                .await
                                .map_err(api_error_to_mcp)?,
                        },
                    )
                }
                DictToolArgs::DeleteType { id } => (
                    ToolExecutionMode::Apply,
                    DictToolResult::Deleted {
                        id: domain
                            .delete_dict_type(id)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
                DictToolArgs::CreateData { operator, data } => {
                    let operator = operator_name(operator);
                    (
                        ToolExecutionMode::Apply,
                        DictToolResult::Data {
                            item: domain
                                .create_dict_data(data, &operator)
                                .await
                                .map_err(api_error_to_mcp)?,
                        },
                    )
                }
                DictToolArgs::UpdateData { id, operator, data } => {
                    let operator = operator_name(operator);
                    (
                        ToolExecutionMode::Apply,
                        DictToolResult::Data {
                            item: domain
                                .update_dict_data(id, data, &operator)
                                .await
                                .map_err(api_error_to_mcp)?,
                        },
                    )
                }
                DictToolArgs::DeleteData { id } => (
                    ToolExecutionMode::Apply,
                    DictToolResult::Deleted {
                        id: domain
                            .delete_dict_data(id)
                            .await
                            .map_err(api_error_to_mcp)?,
                    },
                ),
            };
            Ok(Json(DictToolResponse { mode, result }))
        })
    }

    #[tool(
        name = "sql_query_readonly",
        description = "Execute one read-only SQL query for complex reads that cannot be expressed by table_query"
    )]
    async fn sql_query_readonly_tool(
        &self,
        Parameters(args): Parameters<SqlQueryReadonlyArgs>,
    ) -> Result<Json<SqlQueryReadonlyResult>, McpError> {
        tool_result!("sql_query_readonly", {
            let sql = normalize_readonly_sql(&args.sql)?;
            let search_path_schema = normalize_explicit_schema(args.schema.as_deref())?;
            let limit = args
                .limit
                .unwrap_or(DEFAULT_SQL_QUERY_LIMIT)
                .clamp(1, MAX_SQL_QUERY_LIMIT);
            let params = convert_sql_params(&args.params)?;
            let wrapped_sql = format!(
                "SELECT * FROM ({sql}) AS {} LIMIT {limit}",
                quote_identifier(READONLY_SQL_SUBQUERY_ALIAS)
            );

            let rows = self
                .db()
                .transaction_with_config(
                    move |txn| {
                        let search_path_schema = search_path_schema.clone();
                        let statement = Statement::from_sql_and_values(
                            DbBackend::Postgres,
                            wrapped_sql.clone(),
                            params.clone(),
                        );
                        Box::pin(async move {
                            if let Some(schema) = search_path_schema.as_deref() {
                                txn.execute_raw(search_path_statement(schema))
                                    .await
                                    .map_err(|error| {
                                        sql_tool_db_error("set SQL search_path", error)
                                    })?;
                            }
                            SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(
                                statement,
                            )
                            .all(txn)
                            .await
                            .map_err(|error| {
                                sql_tool_db_error("execute read-only SQL query", error)
                            })
                        })
                    },
                    None,
                    Some(AccessMode::ReadOnly),
                )
                .await
                .map_err(|error| match error {
                    TransactionError::Connection(error) => {
                        sql_tool_db_error("start read-only SQL transaction", error)
                    }
                    TransactionError::Transaction(error) => error,
                })?;

            let result = SqlQueryReadonlyResult {
                row_count: rows.len() as u64,
                rows,
                limit,
            };
            Ok(Json(result))
        })
    }

    #[tool(
        name = "sql_exec",
        description = "Execute one SQL statement for DDL or data modification. Use sql_query_readonly for reads."
    )]
    async fn sql_exec_tool(
        &self,
        Parameters(args): Parameters<SqlExecArgs>,
    ) -> Result<Json<SqlExecResult>, McpError> {
        tool_result!("sql_exec", {
            let sql = normalize_exec_sql(&args.sql)?;
            let search_path_schema = normalize_explicit_schema(args.schema.as_deref())?;
            let params = convert_sql_params(&args.params)?;

            tracing::warn!(target: "summer_mcp::sql_exec", sql = %sql, "executing raw SQL via MCP sql_exec");

            let rows_affected = self
                .db()
                .transaction(move |txn| {
                    let search_path_schema = search_path_schema.clone();
                    let statement =
                        Statement::from_sql_and_values(DbBackend::Postgres, sql.clone(), params);
                    Box::pin(async move {
                        if let Some(schema) = search_path_schema.as_deref() {
                            txn.execute_raw(search_path_statement(schema))
                                .await
                                .map_err(|error| sql_tool_db_error("set SQL search_path", error))?;
                        }
                        let result = txn
                            .execute_raw(statement)
                            .await
                            .map_err(|error| sql_tool_db_error("execute SQL statement", error))?;
                        Ok(result.rows_affected())
                    })
                })
                .await
                .map_err(|error| match error {
                    TransactionError::Connection(error) => {
                        sql_tool_db_error("start sql_exec transaction", error)
                    }
                    TransactionError::Transaction(error) => error,
                })?;

            Ok(Json(SqlExecResult { rows_affected }))
        })
    }

    #[tool(description = "Fetch one row from a table by primary key")]
    async fn table_get(
        &self,
        Parameters(args): Parameters<TableGetArgs>,
    ) -> Result<Json<TableLookupResult>, McpError> {
        tool_result!("table_get", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema =
                describe_table_for_crud_in_schema(self.db(), &schema_name, &args.table).await?;
            let select_list = readable_select_list(&schema, None)?;
            let mut params = Vec::new();
            let where_clause = build_key_clause(&schema, &args.key, &mut params)?;
            let statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "SELECT {select_list} FROM {} WHERE {where_clause} LIMIT 1",
                    schema.qualified_name()
                ),
                params,
            );

            let item =
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
                    .one(self.db())
                    .await
                    .map_err(|error| {
                        db_error(format!("query row from `{}`", schema.table), error)
                    })?;

            Ok(Json(TableLookupResult {
                schema: schema.schema,
                table: schema.table,
                found: item.is_some(),
                item,
            }))
        })
    }

    #[tool(
        description = "Query rows from a table with runtime schema validation, filters, sorting, and pagination"
    )]
    async fn table_query(
        &self,
        Parameters(args): Parameters<TableQueryArgs>,
    ) -> Result<Json<TableListResult>, McpError> {
        tool_result!("table_query", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema =
                describe_table_for_crud_in_schema(self.db(), &schema_name, &args.table).await?;
            let select_list = readable_select_list(&schema, args.columns.as_deref())?;
            let window = ListWindow::from_args(args.limit, args.offset);

            let (where_clause, count_params) =
                build_filters_clause(&schema, args.filters.as_deref())?;
            let total_statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "SELECT COUNT(*)::bigint AS total FROM {}{}",
                    schema.qualified_name(),
                    where_clause
                        .as_ref()
                        .map(|clause| format!(" WHERE {clause}"))
                        .unwrap_or_default()
                ),
                count_params.clone(),
            );
            let total =
                SelectorRaw::<SelectModel<CountRow>>::from_statement::<CountRow>(total_statement)
                    .one(self.db())
                    .await
                    .map_err(|error| db_error(format!("count rows in `{}`", schema.table), error))?
                    .map(|row: CountRow| row.total.max(0) as u64)
                    .unwrap_or(0);

            let order_clause = build_order_clause(&schema, args.order_by.as_deref())?;
            let items_statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "SELECT {select_list} FROM {}{}{} LIMIT {} OFFSET {}",
                    schema.qualified_name(),
                    where_clause
                        .as_ref()
                        .map(|clause| format!(" WHERE {clause}"))
                        .unwrap_or_default(),
                    order_clause,
                    window.limit,
                    window.offset
                ),
                count_params,
            );
            let items =
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(items_statement)
                    .all(self.db())
                    .await
                    .map_err(|error| {
                        db_error(format!("query rows from `{}`", schema.table), error)
                    })?;

            Ok(Json(TableListResult {
                schema: schema.schema,
                table: schema.table,
                items,
                total,
                limit: window.limit,
                offset: window.offset,
            }))
        })
    }

    #[tool(description = "Insert one row into a table and return the created row")]
    async fn table_insert(
        &self,
        Parameters(args): Parameters<TableInsertArgs>,
    ) -> Result<Json<TableMutationResult>, McpError> {
        tool_result!("table_insert", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema =
                describe_table_for_crud_in_schema(self.db(), &schema_name, &args.table).await?;
            let (columns, values, params) = build_insert_assignments(&schema, &args.values)?;
            let returning = readable_select_list(&schema, None)?;
            let statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "INSERT INTO {} ({columns}) VALUES ({values}) RETURNING {returning}",
                    schema.qualified_name()
                ),
                params,
            );

            let item =
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
                    .one(self.db())
                    .await
                    .map_err(|error| {
                        db_error(format!("insert row into `{}`", schema.table), error)
                    })?;

            Ok(Json(TableMutationResult {
                schema: schema.schema,
                table: schema.table,
                found: item.is_some(),
                changed: item.is_some(),
                item,
            }))
        })
    }

    #[tool(description = "Update one row in a table by primary key and return the latest row")]
    async fn table_update(
        &self,
        Parameters(args): Parameters<TableUpdateArgs>,
    ) -> Result<Json<TableMutationResult>, McpError> {
        tool_result!("table_update", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema =
                describe_table_for_crud_in_schema(self.db(), &schema_name, &args.table).await?;
            let (set_clause, mut params) = build_update_assignments(&schema, &args.values)?;
            let where_clause = build_key_clause(&schema, &args.key, &mut params)?;
            let returning = readable_select_list(&schema, None)?;
            let statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "UPDATE {} SET {set_clause} WHERE {where_clause} RETURNING {returning}",
                    schema.qualified_name()
                ),
                params,
            );

            let item =
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
                    .one(self.db())
                    .await
                    .map_err(|error| {
                        db_error(format!("update row in `{}`", schema.table), error)
                    })?;

            let (found, changed) = if item.is_some() {
                (true, true)
            } else {
                let mut key_params = Vec::new();
                let key_where = build_key_clause(&schema, &args.key, &mut key_params)?;
                let exists_sql = format!(
                    "SELECT 1 AS v FROM {} WHERE {key_where} LIMIT 1",
                    schema.qualified_name()
                );
                let exists = SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(
                    Statement::from_sql_and_values(DbBackend::Postgres, exists_sql, key_params),
                )
                .one(self.db())
                .await
                .map_err(|error| {
                    db_error(
                        format!("check existence of row in `{}`", schema.table),
                        error,
                    )
                })?
                .is_some();
                (exists, false)
            };

            Ok(Json(TableMutationResult {
                schema: schema.schema,
                table: schema.table,
                found,
                changed,
                item,
            }))
        })
    }

    #[tool(description = "Delete one row from a table by primary key")]
    async fn table_delete(
        &self,
        Parameters(args): Parameters<TableDeleteArgs>,
    ) -> Result<Json<TableDeleteResult>, McpError> {
        tool_result!("table_delete", {
            let schema_name = normalize_schema(args.schema.as_deref())?;
            let schema =
                describe_table_for_crud_in_schema(self.db(), &schema_name, &args.table).await?;
            let mut params = Vec::new();
            let where_clause = build_key_clause(&schema, &args.key, &mut params)?;
            let statement = Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "DELETE FROM {} WHERE {where_clause} RETURNING 1 AS deleted",
                    schema.qualified_name()
                ),
                params,
            );

            let deleted =
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
                    .one(self.db())
                    .await
                    .map_err(|error| {
                        db_error(format!("delete row from `{}`", schema.table), error)
                    })?
                    .is_some();

            let found = if deleted {
                true
            } else {
                let mut key_params = Vec::new();
                let key_where = build_key_clause(&schema, &args.key, &mut key_params)?;
                let exists_sql = format!(
                    "SELECT 1 AS v FROM {} WHERE {key_where} LIMIT 1",
                    schema.qualified_name()
                );
                SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(
                    Statement::from_sql_and_values(DbBackend::Postgres, exists_sql, key_params),
                )
                .one(self.db())
                .await
                .map_err(|error| {
                    db_error(
                        format!("check existence of row in `{}`", schema.table),
                        error,
                    )
                })?
                .is_some()
            };

            Ok(Json(TableDeleteResult {
                schema: schema.schema,
                table: schema.table,
                found,
                deleted,
                rows_affected: u64::from(deleted),
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{McpConfig, McpHttpMode, McpTransport};
    use crate::table_tools::query_builder::TableFilterInput;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn router_exposes_small_generic_surface() {
        let tools = AdminMcpServer::tool_router().list_all();
        let mut names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        names.sort_unstable();

        assert_eq!(
            names,
            vec![
                "dict_tool",
                "generate_admin_module_from_table",
                "generate_entity_from_table",
                "generate_frontend_api_from_table",
                "generate_frontend_bundle_from_table",
                "generate_frontend_page_from_table",
                "menu_tool",
                "schema_describe_table",
                "schema_list_tables",
                "server_capabilities",
                "sql_exec",
                "sql_query_readonly",
                "table_delete",
                "table_get",
                "table_insert",
                "table_query",
                "table_update",
                "upgrade_entity_enums_from_table",
            ]
        );
    }

    #[test]
    fn table_query_args_accept_structured_and_shorthand_filters() {
        let args: TableQueryArgs = serde_json::from_value(json!({
            "table": "sys_role",
            "filters": [
                {"column":"id","op":"eq","value": 1},
                {"or": [
                    {"column":"status","op":"eq","value": 1},
                    {"column":"status","op":"eq","value": 2}
                ]},
                "role_name ilike admin"
            ]
        }))
        .unwrap();

        let filters = args.filters.unwrap();
        assert_eq!(filters.len(), 3);
        assert!(matches!(filters[0], TableFilterInput::Structured(_)));
        assert!(matches!(filters[1], TableFilterInput::Group(_)));
        assert!(matches!(filters[2], TableFilterInput::Shorthand(_)));
    }

    #[test]
    fn sql_args_accept_typed_params() {
        let args: SqlQueryReadonlyArgs = serde_json::from_value(json!({
            "sql": "select * from sys_role where id = $1",
            "params": [
                {"kind":"bigint","value":"13"}
            ]
        }))
        .unwrap();

        assert_eq!(args.params.len(), 1);
    }

    #[test]
    fn sql_and_table_args_accept_optional_pg_schema() {
        let list_args: ListTablesArgs = serde_json::from_value(json!({
            "schema": "tenant_demo",
        }))
        .unwrap();
        assert_eq!(list_args.schema.as_deref(), Some("tenant_demo"));

        let default_list_args: ListTablesArgs = serde_json::from_value(json!({})).unwrap();
        assert!(default_list_args.schema.is_none());

        let query_args: SqlQueryReadonlyArgs = serde_json::from_value(json!({
            "schema": "tenant_demo",
            "sql": "select * from sys_role",
        }))
        .unwrap();
        assert_eq!(query_args.schema.as_deref(), Some("tenant_demo"));

        let exec_args: SqlExecArgs = serde_json::from_value(json!({
            "schema": "tenant_demo",
            "sql": "update sys_role set enabled = true",
        }))
        .unwrap();
        assert_eq!(exec_args.schema.as_deref(), Some("tenant_demo"));

        let table_args: TableQueryArgs = serde_json::from_value(json!({
            "schema": "tenant_demo",
            "table": "sys_role",
        }))
        .unwrap();
        assert_eq!(table_args.schema.as_deref(), Some("tenant_demo"));
    }

    #[test]
    fn listed_tool_schemas_are_compatible_with_strict_mcp_clients() {
        for (index, tool) in AdminMcpServer::tool_router().list_all().iter().enumerate() {
            assert_eq!(
                tool.input_schema.get("type"),
                Some(&json!("object")),
                "tool #{index} `{}` inputSchema root type must be object: {}",
                tool.name,
                serde_json::to_string(&tool.input_schema).unwrap()
            );

            if let Some(output_schema) = &tool.output_schema {
                assert_eq!(
                    output_schema.get("type"),
                    Some(&json!("object")),
                    "tool #{index} `{}` outputSchema root type must be object",
                    tool.name
                );
                assert_top_level_properties_are_schema_objects(&tool.name, output_schema.as_ref());
            }
        }
    }

    fn assert_top_level_properties_are_schema_objects(
        tool_name: &str,
        schema: &serde_json::Map<String, serde_json::Value>,
    ) {
        let Some(serde_json::Value::Object(properties)) = schema.get("properties") else {
            return;
        };

        for (property, property_schema) in properties {
            assert!(
                property_schema.is_object(),
                "tool `{tool_name}` outputSchema property `{property}` must be a schema object, got {property_schema}"
            );
        }
    }

    #[tokio::test]
    async fn sql_exec_sets_search_path_before_user_sql_when_schema_is_specified() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 0,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let server = AdminMcpServer::new(&McpConfig::default(), db.clone());

        let Json(result) = server
            .sql_exec_tool(Parameters(SqlExecArgs {
                schema: Some("tenant_demo".to_string()),
                sql: "update sys_role set enabled = true".to_string(),
                params: vec![],
            }))
            .await
            .unwrap();

        assert_eq!(result.rows_affected, 1);

        let logs = db.into_transaction_log();
        let statements = logs[0].statements();
        assert_eq!(statements[0].sql, "BEGIN");
        assert_eq!(
            statements[1].sql,
            "SET LOCAL search_path TO \"tenant_demo\", pg_catalog"
        );
        assert_eq!(statements[2].sql, "update sys_role set enabled = true");
        assert_eq!(statements[3].sql, "COMMIT");
    }

    #[test]
    fn generator_args_accept_explicit_field_contracts() {
        let args: GenerateFrontendBundleFromTableArgs = serde_json::from_value(json!({
            "table": "sys_config",
            "query_fields": ["config_name", "config_key"],
            "create_fields": ["config_name", "config_key", "config_value"],
            "update_fields": ["config_value"],
            "list_fields": ["config_name", "config_key", "enabled"],
            "detail_fields": ["config_name", "config_key", "config_value", "remark"],
            "search_fields": ["config_name"],
            "table_fields": ["config_name", "enabled"],
            "form_fields": ["config_name", "config_value"]
        }))
        .unwrap();

        assert_eq!(
            args.query_fields,
            Some(vec!["config_name".to_string(), "config_key".to_string()])
        );
        assert_eq!(
            args.create_fields,
            Some(vec![
                "config_name".to_string(),
                "config_key".to_string(),
                "config_value".to_string(),
            ])
        );
        assert_eq!(args.update_fields, Some(vec!["config_value".to_string()]));
        assert_eq!(
            args.list_fields,
            Some(vec![
                "config_name".to_string(),
                "config_key".to_string(),
                "enabled".to_string(),
            ])
        );
        assert_eq!(
            args.detail_fields,
            Some(vec![
                "config_name".to_string(),
                "config_key".to_string(),
                "config_value".to_string(),
                "remark".to_string(),
            ])
        );
    }

    #[tokio::test]
    async fn server_capabilities_reports_runtime_and_database_health() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![row([("table_name", "sys_user".into())])]])
            .into_connection();

        let mut config = McpConfig {
            enabled: true,
            transport: McpTransport::Http,
            http_mode: McpHttpMode::Embedded,
            path: "/mcp".to_string(),
            port: 9090,
            ..McpConfig::default()
        };
        config.default_database_url = Some("postgres://demo".to_string());

        let server = AdminMcpServer::new(&config, db);
        let Json(result) = server.server_capabilities().await.unwrap();

        assert_eq!(result.health.status, ServerHealthStatus::Ok);
        assert!(result.health.database.connected);
        assert_eq!(result.health.database.public_table_count, Some(1));
        assert_eq!(result.runtime.transport, "http");
        assert_eq!(result.runtime.http_mode, "embedded");
        assert!(result.runtime.default_database_url_available);
        assert!(
            result
                .capabilities
                .tools
                .contains(&"server_capabilities".to_string())
        );
        assert_eq!(
            result.capabilities.prompts,
            vec![
                "discover_table_workflow".to_string(),
                "generate_crud_bundle_workflow".to_string(),
                "rollout_menu_dict_workflow".to_string(),
            ]
        );
        assert_eq!(
            result.capabilities.generators.frontend_target_presets,
            vec!["summer_mcp".to_string(), "art_design_pro".to_string()]
        );
    }

    fn row<const N: usize>(entries: [(&str, Value); N]) -> BTreeMap<String, Value> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect()
    }
}
