use rmcp::schemars;
use sea_orm::JsonValue;
use serde::{Deserialize, Serialize};
use summer_domain::{
    dict::{DictBundleSpec, DictBundleSyncResult},
    menu::{MenuConfigSpec, MenuConfigSyncResult},
};
use summer_system_model::vo::{
    sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo},
    sys_menu::{MenuTreeVo, MenuVo},
};

use crate::{
    output_contract::{ArtifactBundleSummary, ToolExecutionMode},
    tools::{
        entity_enum_upgrader::EntityEnumUpgradePlan, enum_semantics::EnumDraftSpec,
        validation::GenerationValidationSummary,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ListTablesResult {
    pub schema: String,
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TableLookupResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub item: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TableListResult {
    pub schema: String,
    pub table: String,
    pub items: Vec<JsonValue>,
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TableMutationResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub changed: bool,
    pub item: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct TableDeleteResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub deleted: bool,
    pub rows_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct SqlQueryReadonlyResult {
    pub rows: Vec<JsonValue>,
    pub row_count: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct SqlExecResult {
    pub rows_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct GenerateEntityFromTableResult {
    pub table: String,
    pub entity_file: String,
    pub mod_file: String,
    pub overwritten: bool,
    pub database_schema: String,
    pub cli_bin: String,
    pub enum_upgrade_changed: bool,
    pub enum_upgrade_fields: Vec<String>,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct GenerateAdminModuleFromTableResult {
    pub table: String,
    pub route_base: String,
    pub router_file: String,
    pub service_file: String,
    pub dto_file: String,
    pub vo_file: String,
    pub updated_mod_files: Vec<String>,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct GenerateFrontendApiFromTableResult {
    pub table: String,
    pub route_base: String,
    pub namespace: String,
    pub api_file: String,
    pub api_type_file: String,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct GenerateFrontendPageFromTableResult {
    pub table: String,
    pub route_base: String,
    pub api_import_path: String,
    pub api_namespace: String,
    pub page_dir: String,
    pub types_file: String,
    pub index_file: String,
    pub search_file: String,
    pub form_panel_file: String,
    pub required_dict_types: Vec<String>,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct GenerateFrontendBundleFromTableResult {
    pub table: String,
    pub route_base: String,
    pub api_namespace: String,
    pub api_import_path: String,
    pub frontend_root_dir: String,
    pub api_file: String,
    pub api_type_file: String,
    pub page_dir: String,
    pub types_file: String,
    pub index_file: String,
    pub search_file: String,
    pub form_panel_file: String,
    pub required_dict_types: Vec<String>,
    pub enum_drafts: Vec<EnumDraftSpec>,
    pub dict_bundle_drafts: Vec<DictBundleSpec>,
    pub menu_config_draft: MenuConfigSpec,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct UpgradeEntityEnumsFromTableResult {
    pub mode: ToolExecutionMode,
    pub table: String,
    pub route_base: String,
    pub entity_file: String,
    pub changed: bool,
    pub plan: EntityEnumUpgradePlan,
    pub rendered_source: String,
    pub artifacts: ArtifactBundleSummary,
    pub validation: Option<GenerationValidationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub(crate) struct ExportArtifactsResult {
    pub output_dir: String,
    pub spec_file: String,
    pub plan_file: String,
    pub artifacts: ArtifactBundleSummary,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum MenuToolResult {
    Tree {
        items: Vec<MenuTreeVo>,
    },
    Menu {
        item: MenuVo,
    },
    ConfigSync {
        sync: MenuConfigSyncResult,
    },
    ConfigExport {
        export: ExportArtifactsResult,
        sync: MenuConfigSyncResult,
    },
    Deleted {
        id: i64,
    },
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct MenuToolResponse {
    pub mode: ToolExecutionMode,
    pub result: MenuToolResult,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum DictToolResult {
    TypeList {
        items: Vec<DictTypeVo>,
    },
    DataList {
        items: Vec<DictDataVo>,
    },
    SimpleDataList {
        items: Vec<DictDataSimpleVo>,
    },
    AllData {
        data: std::collections::HashMap<String, Vec<DictDataSimpleVo>>,
    },
    Type {
        item: DictTypeVo,
    },
    Data {
        item: DictDataVo,
    },
    BundleSync {
        sync: DictBundleSyncResult,
    },
    BundleExport {
        export: ExportArtifactsResult,
        sync: DictBundleSyncResult,
    },
    Deleted {
        id: i64,
    },
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct DictToolResponse {
    pub mode: ToolExecutionMode,
    pub result: DictToolResult,
}
