use rmcp::{
    handler::server::{router::prompt::PromptRouter, wrapper::Parameters},
    model::{GetPromptResult, PromptMessage, PromptMessageRole},
    prompt, prompt_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::server::{AdminMcpServer, schema_table_resource, schema_tables_resource};

pub(crate) const DISCOVER_TABLE_WORKFLOW: &str = "discover_table_workflow";
pub(crate) const GENERATE_CRUD_BUNDLE_WORKFLOW: &str = "generate_crud_bundle_workflow";
pub(crate) const ROLLOUT_MENU_DICT_WORKFLOW: &str = "rollout_menu_dict_workflow";

pub(crate) const DISCOVER_TABLE_WORKFLOW_TITLE: &str = "Discover Table Workflow";
pub(crate) const GENERATE_CRUD_BUNDLE_WORKFLOW_TITLE: &str = "Generate CRUD Bundle Workflow";
pub(crate) const ROLLOUT_MENU_DICT_WORKFLOW_TITLE: &str = "Rollout Menu/Dict Workflow";

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub(crate) struct DiscoverTableWorkflowArgs {
    /// Optional target table name, for example sys_user.
    pub table: Option<String>,
    /// Optional short task goal, for example list active users.
    pub goal: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct GenerateCrudBundleWorkflowArgs {
    /// Target table name, for example sys_user.
    pub table: String,
    /// Optional temp output dir, for example /tmp/sys-user-crud.
    pub output_dir: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub(crate) struct RolloutMenuDictWorkflowArgs {
    /// Optional source table name, for example sys_user.
    pub table: Option<String>,
    /// Optional route base, for example user.
    pub route_base: Option<String>,
}

pub(crate) fn render_discover_table_workflow(args: DiscoverTableWorkflowArgs) -> GetPromptResult {
    let mut messages = vec![
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            format!(
                "Follow the Summerrs Admin discovery workflow{}{}:\n\
                 1. Read `schema://tables` first to confirm the live table list.\n\
                 2. If the target table is known, read `schema://table/{{table}}` before querying or mutating data.\n\
                 3. Prefer `table_get` and `table_query` for reads; use `sql_query_readonly` only when the shape is too complex for the generic DSL.\n\
                 4. Prefer `table_insert`, `table_update`, and `table_delete` for simple row operations; use `sql_exec` only for explicit DDL/DML that cannot be expressed by table tools.\n\
                 5. Do not guess column names, hidden fields, enum values, or primary keys before reading schema resources.",
                args.table
                    .as_deref()
                    .map(|table| format!(" for table `{table}`"))
                    .unwrap_or_default(),
                args.goal
                    .as_deref()
                    .map(|goal| format!(" and goal `{goal}`"))
                    .unwrap_or_default()
            ),
        ),
        PromptMessage::new_resource_link(PromptMessageRole::Assistant, schema_tables_resource()),
    ];

    if let Some(table) = args.table {
        messages.push(PromptMessage::new_resource_link(
            PromptMessageRole::Assistant,
            schema_table_resource(&table),
        ));
    }

    GetPromptResult::new(messages).with_description(
        "Recommended discovery-first workflow before calling generic table tools.",
    )
}

pub(crate) fn render_generate_crud_bundle_workflow(
    args: GenerateCrudBundleWorkflowArgs,
) -> GetPromptResult {
    let table = args.table;
    let output_dir = args
        .output_dir
        .unwrap_or_else(|| format!("/tmp/{table}-crud-bundle"));

    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            format!(
                "Follow this Summerrs Admin CRUD generation workflow for table `{table}`:\n\
                 1. Read `schema://table/{table}` first and confirm the primary key, hidden fields, enum values, and nullable columns.\n\
                 2. Generate the SeaORM entity with `generate_entity_from_table`; default comment/native enum semantics will be upgraded into `DeriveActiveEnum` automatically.\n\
                 3. If enum labels are non-ASCII or you care about readable Rust names, pass `enum_name_overrides` and `variant_name_overrides` directly to `generate_entity_from_table`.\n\
                 4. If you need to preview the enum plan before apply, or refine overrides separately, call `upgrade_entity_enums_from_table` explicitly before moving on.\n\
                 5. Generate the backend CRUD skeleton with `generate_admin_module_from_table`, writing to `{output_dir}` first if you want a safe temp preview.\n\
                 6. Generate the frontend bundle with `generate_frontend_bundle_from_table`, also targeting `{output_dir}` first when you want to inspect output before moving files.\n\
                 7. Review the returned `menu_config_draft` and `dict_bundle_drafts` instead of hand-writing menu or dict SQL.\n\
                 8. If the generated drafts look correct, hand them to `menu_tool` and `dict_tool` using `plan_*` or `export_*` first, then `apply_*` when you really want to persist them."
            ),
        ),
        PromptMessage::new_resource_link(
            PromptMessageRole::Assistant,
            schema_table_resource(&table),
        ),
    ])
    .with_description("End-to-end workflow for generating a CRUD bundle from one database table.")
}

pub(crate) fn render_rollout_menu_dict_workflow(
    args: RolloutMenuDictWorkflowArgs,
) -> GetPromptResult {
    let subject = match (args.table.as_deref(), args.route_base.as_deref()) {
        (Some(table), Some(route_base)) => {
            format!(" for table `{table}` with route `{route_base}`")
        }
        (Some(table), None) => format!(" for table `{table}`"),
        (None, Some(route_base)) => format!(" for route `{route_base}`"),
        (None, None) => String::new(),
    };

    GetPromptResult::new(vec![PromptMessage::new_text(
        PromptMessageRole::Assistant,
        format!(
            "Follow this menu and dictionary rollout workflow{subject}:\n\
             1. Treat `menu_tool` and `dict_tool` as the business entry points; do not insert directly into `sys_menu`, `sys_dict_type`, or `sys_dict_data` unless you are explicitly doing low-level repair.\n\
             2. Start with `plan_config` / `plan_bundle` to inspect what will be created or updated.\n\
             3. If you need an artifact for review, use `export_config` / `export_bundle` to write JSON plan files into a temp directory.\n\
             4. Only call `apply_config` / `apply_bundle` after the generated drafts are confirmed.\n\
             5. After apply, verify with `menu_tool.list_tree` or `dict_tool.get_by_type` instead of assuming the write succeeded."
        ),
    )])
    .with_description("Workflow for staging, reviewing, and applying menu/dictionary business configuration.")
}

pub(crate) fn build_prompt_router() -> PromptRouter<AdminMcpServer> {
    let mut router = AdminMcpServer::prompt_router();

    if let Some(route) = router.map.get_mut(DISCOVER_TABLE_WORKFLOW) {
        route.attr = route.attr.clone().with_title(DISCOVER_TABLE_WORKFLOW_TITLE);
    }
    if let Some(route) = router.map.get_mut(GENERATE_CRUD_BUNDLE_WORKFLOW) {
        route.attr = route
            .attr
            .clone()
            .with_title(GENERATE_CRUD_BUNDLE_WORKFLOW_TITLE);
    }
    if let Some(route) = router.map.get_mut(ROLLOUT_MENU_DICT_WORKFLOW) {
        route.attr = route
            .attr
            .clone()
            .with_title(ROLLOUT_MENU_DICT_WORKFLOW_TITLE);
    }

    router
}

#[prompt_router]
impl AdminMcpServer {
    #[prompt(
        name = "discover_table_workflow",
        description = "Inspect schema resources first, then choose the safest table or SQL path."
    )]
    async fn discover_table_workflow(
        &self,
        Parameters(args): Parameters<DiscoverTableWorkflowArgs>,
    ) -> GetPromptResult {
        render_discover_table_workflow(args)
    }

    #[prompt(
        name = "generate_crud_bundle_workflow",
        description = "Generate entity, backend CRUD skeleton, and frontend bundle for a table."
    )]
    async fn generate_crud_bundle_workflow(
        &self,
        Parameters(args): Parameters<GenerateCrudBundleWorkflowArgs>,
    ) -> GetPromptResult {
        render_generate_crud_bundle_workflow(args)
    }

    #[prompt(
        name = "rollout_menu_dict_workflow",
        description = "Review generated menu and dictionary drafts, then stage or apply them."
    )]
    async fn rollout_menu_dict_workflow(
        &self,
        Parameters(args): Parameters<RolloutMenuDictWorkflowArgs>,
    ) -> GetPromptResult {
        render_rollout_menu_dict_workflow(args)
    }
}

#[cfg(test)]
mod tests {
    use crate::server::AdminMcpServer;

    use super::{
        DiscoverTableWorkflowArgs, GenerateCrudBundleWorkflowArgs, RolloutMenuDictWorkflowArgs,
        build_prompt_router, render_discover_table_workflow, render_generate_crud_bundle_workflow,
        render_rollout_menu_dict_workflow,
    };

    #[test]
    fn discover_prompt_includes_schema_resources() {
        let result = render_discover_table_workflow(DiscoverTableWorkflowArgs {
            table: Some("sys_user".to_string()),
            goal: Some("list active users".to_string()),
        });

        assert_eq!(result.messages.len(), 3);
    }

    #[test]
    fn generate_prompt_uses_default_output_dir() {
        let result = render_generate_crud_bundle_workflow(GenerateCrudBundleWorkflowArgs {
            table: "sys_user".to_string(),
            output_dir: None,
        });

        let rmcp::model::PromptMessageContent::Text { text } = &result.messages[0].content else {
            panic!("expected text content");
        };
        assert!(text.contains("/tmp/sys_user-crud-bundle"));
    }

    #[test]
    fn rollout_prompt_mentions_business_tools() {
        let result = render_rollout_menu_dict_workflow(RolloutMenuDictWorkflowArgs {
            table: Some("sys_user".to_string()),
            route_base: Some("user".to_string()),
        });

        let rmcp::model::PromptMessageContent::Text { text } = &result.messages[0].content else {
            panic!("expected text content");
        };
        assert!(text.contains("menu_tool"));
        assert!(text.contains("dict_tool"));
    }

    #[test]
    fn workflow_prompt_catalog_comes_from_macro_router() {
        let prompts = build_prompt_router().list_all();
        let names: Vec<String> = prompts.into_iter().map(|prompt| prompt.name).collect();
        assert_eq!(
            names,
            vec![
                "discover_table_workflow",
                "generate_crud_bundle_workflow",
                "rollout_menu_dict_workflow",
            ]
        );
    }

    #[test]
    fn generate_crud_prompt_marks_table_as_required() {
        let attr = AdminMcpServer::generate_crud_bundle_workflow_prompt_attr();
        let args = attr.arguments.expect("prompt args should exist");
        let table = args
            .iter()
            .find(|arg| arg.name == "table")
            .expect("table arg should exist");
        assert_eq!(table.required, Some(true));
    }
}
