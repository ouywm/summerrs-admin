use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use rmcp::{
    ErrorData as McpError, Json, handler::server::wrapper::Parameters, schemars, tool, tool_router,
};
use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, JsonValue,
    SelectModel, SelectorRaw, Statement, TransactionError, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use summer_common::error::ApiErrors;
use summer_domain::{
    dict::{DictBundleSpec, DictBundleSyncResult},
    menu::{MenuConfigSpec, MenuConfigSyncResult},
};
use summer_system_model::{
    dto::sys_dict::{
        CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto,
        UpdateDictDataDto, UpdateDictTypeDto,
    },
    dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto},
    vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo},
    vo::sys_menu::{MenuTreeVo, MenuVo},
};

use crate::{
    error_model::{internal_error, invalid_params_error, normalize_tool_error},
    output_contract::{
        ArtifactBundleSummary, ArtifactMode, ToolExecutionMode, build_artifact_bundle,
        generator_artifact_mode,
    },
    prompts,
    server::AdminMcpServer,
    table_tools::{
        query_builder::{
            TableFilterInput, TableSortInput, build_filters_clause, build_insert_assignments,
            build_key_clause, build_order_clause, build_update_assignments,
        },
        schema::{
            TableSchema, db_error, describe_table, describe_table_for_crud,
            ensure_valid_identifier, list_tables, quote_identifier, readable_select_list,
        },
        sql_scanner::{
            SqlParamInput, convert_sql_params, normalize_exec_sql, normalize_readonly_sql,
        },
    },
    tools::{
        admin_module_generator::{AdminModuleGenerator, GenerateAdminModuleRequest},
        entity_enum_upgrader::{EntityEnumUpgradePlan, EntityEnumUpgrader},
        entity_generator::{EntityGenerator, GenerateEntityRequest},
        enum_semantics::EnumDraftSpec,
        frontend_api_generator::{FrontendApiGenerator, GenerateFrontendApiRequest},
        frontend_bundle_generator::{FrontendBundleGenerator, GenerateFrontendBundleRequest},
        frontend_page_generator::{
            FrontendFieldUiHint, FrontendFieldUiMeta, FrontendPageGenerator,
            GenerateFrontendPageRequest,
        },
        frontend_target::FrontendTargetPreset,
        generation_context::CrudFieldSelection,
        support::{
            error_chain_message, resolve_output_dir, sanitize_file_stem, workspace_root,
            write_pretty_json_file,
        },
        validation::{GenerationValidationSummary, validate_frontend_target_output},
    },
};

const DEFAULT_LIST_LIMIT: u64 = 20;
const MAX_LIST_LIMIT: u64 = 100;
const DEFAULT_SQL_QUERY_LIMIT: u64 = 200;
const MAX_SQL_QUERY_LIMIT: u64 = 1_000;
const READONLY_SQL_SUBQUERY_ALIAS: &str = "__summer_mcp_readonly";

type JsonMap = BTreeMap<String, JsonValue>;

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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ListTablesResult {
    pub schema: String,
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub struct TableLookupResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub item: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub struct TableListResult {
    pub schema: String,
    pub table: String,
    pub items: Vec<JsonValue>,
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub struct TableMutationResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub changed: bool,
    pub item: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct TableDeleteResult {
    pub schema: String,
    pub table: String,
    pub found: bool,
    pub deleted: bool,
    pub rows_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub struct SqlQueryReadonlyResult {
    pub rows: Vec<JsonValue>,
    pub row_count: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct SqlExecResult {
    pub rows_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct GenerateEntityFromTableResult {
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
pub struct GenerateAdminModuleFromTableResult {
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
pub struct GenerateFrontendApiFromTableResult {
    pub table: String,
    pub route_base: String,
    pub namespace: String,
    pub api_file: String,
    pub api_type_file: String,
    pub artifacts: ArtifactBundleSummary,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct GenerateFrontendPageFromTableResult {
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
pub struct GenerateFrontendBundleFromTableResult {
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
pub struct UpgradeEntityEnumsFromTableResult {
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
pub struct ExportArtifactsResult {
    pub output_dir: String,
    pub spec_file: String,
    pub plan_file: String,
    pub artifacts: ArtifactBundleSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerHealthStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ServerCapabilitiesResult {
    pub health: ServerHealthSummary,
    pub server: ServerIdentitySummary,
    pub runtime: ServerRuntimeSummary,
    pub capabilities: ServerCapabilityCatalog,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ServerHealthSummary {
    pub status: ServerHealthStatus,
    pub database: DatabaseHealthSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct DatabaseHealthSummary {
    pub backend: String,
    pub connected: bool,
    pub public_table_count: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ServerIdentitySummary {
    pub name: String,
    pub version: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ServerRuntimeSummary {
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
pub struct ServerCapabilityCatalog {
    pub tools: Vec<String>,
    pub prompts: Vec<String>,
    pub resources: Vec<ResourceCapabilitySummary>,
    pub resource_templates: Vec<ResourceTemplateCapabilitySummary>,
    pub generators: GeneratorCapabilitySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ResourceCapabilitySummary {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ResourceTemplateCapabilitySummary {
    pub uri_template: String,
    pub name: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct GeneratorCapabilitySummary {
    pub backend_generators: Vec<String>,
    pub frontend_generators: Vec<String>,
    pub frontend_target_presets: Vec<String>,
    pub supports_temp_output_dir: bool,
    pub returns_menu_dict_drafts: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MenuToolResult {
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
pub struct MenuToolResponse {
    pub mode: ToolExecutionMode,
    pub result: MenuToolResult,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DictToolResult {
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
pub struct DictToolResponse {
    pub mode: ToolExecutionMode,
    pub result: DictToolResult,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct DescribeTableArgs {
    /// 需要查看的表名
    table: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TableQueryArgs {
    /// 目标表名
    table: String,
    /// 需要返回的列，为空时返回所有可读列
    columns: Option<Vec<String>>,
    /// 过滤条件列表。支持结构化对象：
    /// [{"column":"id","op":"eq","value":1}]
    /// 也支持结构化分组：
    /// [{"or":[{"column":"status","op":"eq","value":1},{"column":"status","op":"eq","value":2}]}]
    /// 也支持简写字符串：
    /// ["id = 1", "role_name ilike admin", "status in [1,2,3]", "create_time between [\"2026-01-01\",\"2026-01-31\"]"]
    filters: Option<Vec<TableFilterInput>>,
    /// 排序条件，支持 [{"column":"id","direction":"desc"}] 或 ["id desc"]
    order_by: Option<Vec<TableSortInput>>,
    /// 返回条数，默认 20，最大 100
    limit: Option<u64>,
    /// 跳过条数，默认 0
    offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TableGetArgs {
    /// 目标表名
    table: String,
    /// 主键对象，支持联合主键
    key: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TableInsertArgs {
    /// 目标表名
    table: String,
    /// 新纪录字段值
    values: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TableUpdateArgs {
    /// 目标表名
    table: String,
    /// 主键对象，支持联合主键
    key: JsonMap,
    /// 需要更新的字段值
    values: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TableDeleteArgs {
    /// 目标表名
    table: String,
    /// 主键对象，支持联合主键
    key: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SqlQueryReadonlyArgs {
    /// 只读 SQL，当前仅允许单条 SELECT / WITH ... SELECT 语句
    sql: String,
    /// PostgreSQL 位置参数，对应 $1、$2 ...
    /// 推荐直接传 JSON 原生类型，例如 [13, true, "admin"]。
    /// 如果客户端只能传字符串，可显式传类型对象，例如：
    /// [{"kind":"bigint","value":"13"}]
    #[serde(default)]
    params: Vec<SqlParamInput>,
    /// 服务端返回行数上限，默认 200，最大 1000
    limit: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SqlExecArgs {
    /// 执行 SQL，允许单条 DDL / DML / 管理语句
    sql: String,
    /// PostgreSQL 位置参数，对应 $1、$2 ...
    /// 推荐直接传 JSON 原生类型，例如 [13, true, "admin"]。
    /// 如果客户端只能传字符串，可显式传类型对象，例如：
    /// [{"kind":"bigint","value":"13"}]
    #[serde(default)]
    params: Vec<SqlParamInput>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GenerateEntityFromTableArgs {
    /// 需要生成 Entity 的表名
    table: String,
    /// 是否覆盖已有 entity 文件
    overwrite: Option<bool>,
    /// 输出目录，默认 crates/model/src/entity
    output_dir: Option<String>,
    /// 数据库连接串；未提供时优先使用 standalone 启动时的 --database-url，再回退到 DATABASE_URL
    database_url: Option<String>,
    /// 数据库 schema，默认 public
    database_schema: Option<String>,
    /// sea-orm-cli 可执行文件，默认 sea-orm-cli
    cli_bin: Option<String>,
    /// 可选：覆盖字段的 Rust 枚举名，例如 { "contact_gender": "ContactGender" }
    #[serde(default)]
    enum_name_overrides: BTreeMap<String, String>,
    /// 可选：覆盖枚举值到 Rust 变体名的映射，例如 { "status": { "1": "Enabled" } }
    #[serde(default)]
    variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GenerateAdminModuleFromTableArgs {
    /// 需要生成后台模块骨架的表名
    table: String,
    /// 是否覆盖已有文件
    overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    route_base: Option<String>,
    /// 输出根目录；默认直接写入工作区既有 app/model 目录。
    /// 传入后会改写到：
    /// - <dir>/router
    /// - <dir>/service
    /// - <dir>/dto
    /// - <dir>/vo
    output_dir: Option<String>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    detail_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GenerateFrontendApiFromTableArgs {
    /// 需要生成前端 api/类型声明 的表名
    table: String,
    /// 是否覆盖已有文件
    overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    route_base: Option<String>,
    /// 前端输出根目录。默认 target_preset=summer_mcp，生成 api 到 <dir>/api，类型声明到 <dir>/api_type。
    /// 当 target_preset=art_design_pro 时，生成 api 到 <dir>/src/api，类型声明到 <dir>/src/types/api。
    output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    target_preset: Option<FrontendTargetPreset>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    detail_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GenerateFrontendBundleFromTableArgs {
    /// 需要一次生成 frontend api/类型声明/page 的表名
    table: String,
    /// 是否覆盖已有文件
    overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    route_base: Option<String>,
    /// 前端输出根目录；默认 target_preset=summer_mcp。
    /// 生成结果会写到：
    /// - summer_mcp: <dir>/api, <dir>/api_type, <dir>/views/system/<route-base-kebab>/
    /// - art_design_pro: <dir>/src/api, <dir>/src/types/api, <dir>/src/views/system/<route-base-kebab>/
    output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    target_preset: Option<FrontendTargetPreset>,
    /// 字典绑定，key 为字段名，value 为字典类型编码，例如 { "status": "user_status" }
    #[serde(default)]
    dict_bindings: BTreeMap<String, String>,
    /// 显式字段 UI 提示，供 AI 覆盖默认推断，例如将 avatar 指定为 image/avatar 上传、强制隐藏搜索项等
    #[serde(default)]
    field_hints: BTreeMap<String, FrontendFieldUiHint>,
    /// 结构化字段 UI 元数据。优先级高于 field_hints / dict_bindings，可精确控制搜索、表单、表格组件与可见性
    #[serde(default)]
    field_ui_meta: BTreeMap<String, FrontendFieldUiMeta>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    detail_fields: Option<Vec<String>>,
    /// 显式指定搜索区字段；未传时会按字段语义自动排序并选出所有适合搜索的字段
    search_fields: Option<Vec<String>>,
    /// 显式指定表格列字段，默认自动选择所有可读字段
    table_fields: Option<Vec<String>>,
    /// 显式指定弹窗表单字段，默认自动选择 create/update 字段并做并集
    form_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GenerateFrontendPageFromTableArgs {
    /// 需要生成前端页面骨架的表名
    table: String,
    /// 是否覆盖已有文件
    overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    route_base: Option<String>,
    /// 页面输出目录。默认 target_preset=summer_mcp，最终写到 <output_dir>/<route-base-kebab>/。
    /// 当 target_preset=art_design_pro 时，output_dir 应指向前端项目根目录，最终写到 <output_dir>/src/views/system/<route-base-kebab>/。
    output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    target_preset: Option<FrontendTargetPreset>,
    /// 高级：覆盖页面导入的前端 API 模块路径；默认直接使用生成器产出的 @/api/<route-base-kebab>
    api_import_path: Option<String>,
    /// 高级：覆盖全局 API TypeScript namespace；默认直接使用生成器推导值，例如 Role
    api_namespace: Option<String>,
    /// 高级：仅在适配现有手写业务 API 类型时使用；默认直接使用生成器产出的 <Resource>Vo
    api_list_item_type_name: Option<String>,
    /// 高级：仅在适配现有手写业务 API 类型时使用；默认直接使用生成器产出的 <Resource>DetailVo
    api_detail_type_name: Option<String>,
    /// 字典绑定，key 为字段名，value 为字典类型编码，例如 { "status": "user_status" }
    #[serde(default)]
    dict_bindings: BTreeMap<String, String>,
    /// 显式字段 UI 提示，供 AI 覆盖默认推断，例如将 avatar 指定为 image/avatar 上传、强制隐藏搜索项等
    #[serde(default)]
    field_hints: BTreeMap<String, FrontendFieldUiHint>,
    /// 结构化字段 UI 元数据。优先级高于 field_hints / dict_bindings，可精确控制搜索、表单、表格组件与可见性
    #[serde(default)]
    field_ui_meta: BTreeMap<String, FrontendFieldUiMeta>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    detail_fields: Option<Vec<String>>,
    /// 显式指定搜索区字段；未传时会按字段语义自动排序并选出所有适合搜索的字段
    search_fields: Option<Vec<String>>,
    /// 显式指定表格列字段，默认自动选择所有可读字段
    table_fields: Option<Vec<String>>,
    /// 显式指定弹窗表单字段，默认自动选择 create/update 字段并做并集
    form_fields: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
enum UpgradeEntityEnumsFromTableArgs {
    /// 预览实体枚举升级计划，不写文件
    PlanUpgrade {
        /// 目标表名
        table: String,
        /// 路由基础路径，默认自动从表名推导，例如 sys_user -> user
        route_base: Option<String>,
        /// 实体目录根路径；默认 crates/model/src/entity
        output_dir: Option<String>,
        /// 仅升级指定字段；未传时自动处理全部待升级枚举字段
        fields: Option<Vec<String>>,
        /// 可选：覆盖字段的 Rust 枚举名，例如 { "contact_gender": "ContactGender" }
        #[serde(default)]
        enum_name_overrides: BTreeMap<String, String>,
        /// 可选：覆盖枚举值到 Rust 变体名的映射，例如 { "status": { "1": "Enabled" } }
        #[serde(default)]
        variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
    },
    /// 应用实体枚举升级计划并写回实体文件
    ApplyUpgrade {
        /// 目标表名
        table: String,
        /// 路由基础路径，默认自动从表名推导，例如 sys_user -> user
        route_base: Option<String>,
        /// 实体目录根路径；默认 crates/model/src/entity
        output_dir: Option<String>,
        /// 仅升级指定字段；未传时自动处理全部待升级枚举字段
        fields: Option<Vec<String>>,
        /// 可选：覆盖字段的 Rust 枚举名，例如 { "contact_gender": "ContactGender" }
        #[serde(default)]
        enum_name_overrides: BTreeMap<String, String>,
        /// 可选：覆盖枚举值到 Rust 变体名的映射，例如 { "status": { "1": "Enabled" } }
        #[serde(default)]
        variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
    },
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
enum MenuToolArgs {
    /// 获取管理端菜单树
    ListTree,
    /// 按用户 ID 获取可用菜单树
    GetUserTree { user_id: i64 },
    /// 预览基于树形配置的菜单/按钮变更，不写库
    PlanConfig { config: MenuConfigSpec },
    /// 导出菜单配置和计划结果到目录，不写库
    ExportConfig {
        config: MenuConfigSpec,
        output_dir: String,
    },
    /// 按树形配置批量创建或更新菜单/按钮
    ApplyConfig { config: MenuConfigSpec },
    /// 创建菜单节点
    CreateMenu { data: CreateMenuDto },
    /// 创建按钮权限节点
    CreateButton { data: CreateButtonDto },
    /// 更新菜单节点
    UpdateMenu { id: i64, data: UpdateMenuDto },
    /// 更新按钮权限节点
    UpdateButton { id: i64, data: UpdateButtonDto },
    /// 删除菜单或按钮节点
    DeleteNode { id: i64 },
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
enum DictToolArgs {
    /// 查询字典类型列表
    ListTypes { query: Option<DictTypeQueryDto> },
    /// 查询字典数据列表
    ListData { query: Option<DictDataQueryDto> },
    /// 获取指定字典类型下的启用字典项
    GetByType { dict_type: String },
    /// 获取全部启用字典数据
    GetAllEnabled,
    /// 预览一个字典 bundle 的批量变更，不写库
    PlanBundle { bundle: DictBundleSpec },
    /// 导出字典 bundle 和计划结果到目录，不写库
    ExportBundle {
        bundle: DictBundleSpec,
        output_dir: String,
    },
    /// 按一个字典 bundle 批量创建或更新字典类型和字典项
    ApplyBundle {
        operator: Option<String>,
        bundle: DictBundleSpec,
    },
    /// 创建字典类型
    CreateType {
        operator: Option<String>,
        data: CreateDictTypeDto,
    },
    /// 更新字典类型
    UpdateType {
        id: i64,
        operator: Option<String>,
        data: UpdateDictTypeDto,
    },
    /// 删除字典类型
    DeleteType { id: i64 },
    /// 创建字典数据
    CreateData {
        operator: Option<String>,
        data: CreateDictDataDto,
    },
    /// 更新字典数据
    UpdateData {
        id: i64,
        operator: Option<String>,
        data: UpdateDictDataDto,
    },
    /// 删除字典数据
    DeleteData { id: i64 },
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

fn build_entity_generator_artifacts(
    output_dir: Option<&str>,
    entity_file: &Path,
    mod_file: &Path,
) -> ArtifactBundleSummary {
    let output_root = entity_file.parent().unwrap_or_else(|| Path::new("."));
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        output_root,
        [("entity_file", entity_file), ("mod_file", mod_file)],
    )
}

fn build_admin_generator_artifacts(
    workspace_root: &Path,
    output_dir: Option<&str>,
    router_file: &Path,
    service_file: &Path,
    dto_file: &Path,
    vo_file: &Path,
    mod_files: &[PathBuf],
) -> ArtifactBundleSummary {
    let output_root = if let Some(output_dir) = output_dir {
        resolve_output_dir(workspace_root, Some(output_dir), "")
    } else {
        workspace_root.to_path_buf()
    };
    let mut files = vec![
        ("router_file", router_file),
        ("service_file", service_file),
        ("dto_file", dto_file),
        ("vo_file", vo_file),
    ];
    for mod_file in mod_files {
        files.push(("mod_file", mod_file.as_path()));
    }
    build_artifact_bundle(generator_artifact_mode(output_dir), &output_root, files)
}

fn build_frontend_api_artifacts(
    output_dir: Option<&str>,
    frontend_root_dir: &Path,
    api_file: &Path,
    api_type_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        frontend_root_dir,
        [("api_file", api_file), ("api_type_file", api_type_file)],
    )
}

fn build_frontend_page_artifacts(
    output_dir: Option<&str>,
    page_dir: &Path,
    types_file: &Path,
    index_file: &Path,
    search_file: &Path,
    form_panel_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        page_dir,
        [
            ("types_file", types_file),
            ("index_file", index_file),
            ("search_file", search_file),
            ("form_panel_file", form_panel_file),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn build_frontend_bundle_artifacts(
    output_dir: Option<&str>,
    frontend_root_dir: &Path,
    api_file: &Path,
    api_type_file: &Path,
    types_file: &Path,
    index_file: &Path,
    search_file: &Path,
    form_panel_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        frontend_root_dir,
        [
            ("api_file", api_file),
            ("api_type_file", api_type_file),
            ("types_file", types_file),
            ("index_file", index_file),
            ("search_file", search_file),
            ("form_panel_file", form_panel_file),
        ],
    )
}

fn build_export_artifacts(
    output_root: &Path,
    spec_file: &Path,
    plan_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        ArtifactMode::Export,
        output_root,
        [("spec_file", spec_file), ("plan_file", plan_file)],
    )
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
    async fn schema_list_tables(&self) -> Result<Json<ListTablesResult>, McpError> {
        tool_result!("schema_list_tables", {
            let tables = list_tables(self.db()).await?;
            Ok(Json(ListTablesResult {
                schema: "public".to_string(),
                tables,
            }))
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
            let schema = describe_table(self.db(), &args.table).await?;
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
            let schema = describe_table_for_crud(self.db(), &table).await?;

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
                table,
                route_base,
                output_dir,
                fields,
                enum_name_overrides,
                variant_name_overrides,
            ) = match args {
                UpgradeEntityEnumsFromTableArgs::PlanUpgrade {
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                } => (
                    ToolExecutionMode::Plan,
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                ),
                UpgradeEntityEnumsFromTableArgs::ApplyUpgrade {
                    table,
                    route_base,
                    output_dir,
                    fields,
                    enum_name_overrides,
                    variant_name_overrides,
                } => (
                    ToolExecutionMode::Apply,
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

            let schema = describe_table_for_crud(self.db(), &table).await?;
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
            let schema = describe_table(self.db(), &args.table).await?;
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
            let schema = describe_table(self.db(), &args.table).await?;
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

            let schema = describe_table(self.db(), &args.table).await?;
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
            let schema = describe_table(self.db(), &args.table).await?;
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
                        let statement = Statement::from_sql_and_values(
                            DbBackend::Postgres,
                            wrapped_sql.clone(),
                            params.clone(),
                        );
                        Box::pin(async move {
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
            let params = convert_sql_params(&args.params)?;

            tracing::warn!(target: "summer_mcp::sql_exec", sql = %sql, "executing raw SQL via MCP sql_exec");

            let rows_affected = self
                .db()
                .transaction(move |txn| {
                    let statement =
                        Statement::from_sql_and_values(DbBackend::Postgres, sql.clone(), params);
                    Box::pin(async move {
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
            let schema = describe_table_for_crud(self.db(), &args.table).await?;
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
            let schema = describe_table_for_crud(self.db(), &args.table).await?;
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
            let schema = describe_table_for_crud(self.db(), &args.table).await?;
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
            let schema = describe_table_for_crud(self.db(), &args.table).await?;
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
            let schema = describe_table_for_crud(self.db(), &args.table).await?;
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

async fn export_menu_config_artifacts(
    config: &MenuConfigSpec,
    sync: &MenuConfigSyncResult,
    output_dir: &str,
) -> Result<ExportArtifactsResult, McpError> {
    let output_root = resolve_export_output_dir(output_dir)?;
    let menu_dir = output_root.join("menu");
    let file_stem = menu_export_file_stem(config);
    let spec_file = menu_dir.join(format!("{file_stem}.json"));
    let plan_file = menu_dir.join(format!("{file_stem}.plan.json"));

    write_pretty_json_file(&spec_file, config, "menu export spec").await?;
    write_pretty_json_file(&plan_file, sync, "menu export plan").await?;

    Ok(ExportArtifactsResult {
        output_dir: output_root.display().to_string(),
        spec_file: spec_file.display().to_string(),
        plan_file: plan_file.display().to_string(),
        artifacts: build_export_artifacts(&output_root, &spec_file, &plan_file),
    })
}

async fn export_dict_bundle_artifacts(
    bundle: &DictBundleSpec,
    sync: &DictBundleSyncResult,
    output_dir: &str,
) -> Result<ExportArtifactsResult, McpError> {
    let output_root = resolve_export_output_dir(output_dir)?;
    let dict_dir = output_root.join("dict");
    let file_stem = format!("dict-{}", sanitize_file_stem(&bundle.dict_type));
    let spec_file = dict_dir.join(format!("{file_stem}.json"));
    let plan_file = dict_dir.join(format!("{file_stem}.plan.json"));

    write_pretty_json_file(&spec_file, bundle, "dict export spec").await?;
    write_pretty_json_file(&plan_file, sync, "dict export plan").await?;

    Ok(ExportArtifactsResult {
        output_dir: output_root.display().to_string(),
        spec_file: spec_file.display().to_string(),
        plan_file: plan_file.display().to_string(),
        artifacts: build_export_artifacts(&output_root, &spec_file, &plan_file),
    })
}

fn resolve_export_output_dir(output_dir: &str) -> Result<PathBuf, McpError> {
    if output_dir.trim().is_empty() {
        return Err(invalid_params_error(
            "invalid_output_dir",
            "Invalid output directory",
            Some(
                "Pass a non-empty output_dir. Use an absolute temp path when you want a safe preview.",
            ),
            Some("output_dir cannot be empty".to_string()),
            None,
        ));
    }
    let workspace_root = workspace_root()?;
    Ok(resolve_output_dir(&workspace_root, Some(output_dir), ""))
}

fn menu_export_file_stem(config: &MenuConfigSpec) -> String {
    if let [menu] = config.menus.as_slice() {
        let seed = if menu.path.trim().is_empty() {
            menu.name.as_str()
        } else {
            menu.path.as_str()
        };
        return format!("menu-{}", sanitize_file_stem(seed));
    }
    "menu-config".to_string()
}

fn api_error_to_mcp(error: ApiErrors) -> McpError {
    match error {
        ApiErrors::Internal(error) => internal_error(
            "business_operation_failed",
            "Business operation failed",
            None,
            Some(error_chain_message(error.as_ref())),
            None,
        ),
        ApiErrors::ServiceUnavailable(message) => internal_error(
            "service_unavailable",
            "Service unavailable",
            Some("Check dependent services and retry once the admin application is healthy."),
            Some(message),
            None,
        ),
        ApiErrors::BadRequest(message) => {
            invalid_params_error("bad_request", "Bad request", None, Some(message), None)
        }
        ApiErrors::Unauthorized(message) => invalid_params_error(
            "unauthorized",
            "Unauthorized",
            Some("Check the caller context or authentication setup before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::Forbidden(message) => invalid_params_error(
            "forbidden",
            "Forbidden",
            Some("Check permissions or route guards before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::NotFound(message) => invalid_params_error(
            "entity_not_found",
            "Entity not found",
            Some("Query the current business data first before updating or deleting it."),
            Some(message),
            None,
        ),
        ApiErrors::Conflict(message) => invalid_params_error(
            "conflict",
            "Conflict",
            Some(
                "Inspect current business data first; the write may violate a uniqueness or state rule.",
            ),
            Some(message),
            None,
        ),
        ApiErrors::IncompleteUpload(message) => invalid_params_error(
            "incomplete_upload",
            "Incomplete upload",
            None,
            Some(message),
            None,
        ),
        ApiErrors::ValidationFailed(message) => invalid_params_error(
            "validation_failed",
            "Validation failed",
            Some("Check required fields, enum values, and field formats before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::PayloadTooLarge(message) => invalid_params_error(
            "payload_too_large",
            "Payload too large",
            Some("Reduce payload size or adjust the server request body limit before retrying."),
            Some(message),
            None,
        ),
        ApiErrors::TooManyRequests(message) => invalid_params_error(
            "rate_limited",
            "Too many requests",
            Some("Reduce request frequency and retry later."),
            Some(message),
            None,
        ),
    }
}

fn operator_name(operator: Option<String>) -> String {
    operator
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "mcp".to_string())
}

fn empty_dict_type_query() -> DictTypeQueryDto {
    DictTypeQueryDto {
        dict_name: None,
        dict_type: None,
        status: None,
    }
}

fn empty_dict_data_query() -> DictDataQueryDto {
    DictDataQueryDto {
        dict_type: None,
        dict_label: None,
        status: None,
    }
}

async fn inspect_database_health(db: &DatabaseConnection) -> DatabaseHealthSummary {
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

fn database_backend_name(db: &DatabaseConnection) -> String {
    match db.get_database_backend() {
        DbBackend::MySql => "mysql",
        DbBackend::Postgres => "postgres",
        DbBackend::Sqlite => "sqlite",
        _ => "unknown",
    }
    .to_string()
}

fn tool_catalog() -> Vec<String> {
    let mut names = AdminMcpServer::tool_router()
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn prompt_catalog() -> Vec<String> {
    let mut names = prompts::build_prompt_router()
        .list_all()
        .into_iter()
        .map(|prompt| prompt.name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn resource_catalog() -> Vec<ResourceCapabilitySummary> {
    vec![ResourceCapabilitySummary {
        uri: "schema://tables".to_string(),
        name: "tables".to_string(),
        title: Some("Database Tables".to_string()),
    }]
}

fn resource_template_catalog() -> Vec<ResourceTemplateCapabilitySummary> {
    vec![ResourceTemplateCapabilitySummary {
        uri_template: "schema://table/{table}".to_string(),
        name: "table_schema".to_string(),
        title: Some("Table Schema".to_string()),
    }]
}

fn generator_capability_catalog() -> GeneratorCapabilitySummary {
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

fn sql_tool_db_error(action: impl Into<String>, error: sea_orm::DbErr) -> McpError {
    let action = action.into();
    let detail = error_chain_message(&error);
    if looks_like_sql_param_type_mismatch(&detail) {
        return internal_error(
            "sql_param_type_mismatch",
            "SQL parameter type mismatch",
            Some(
                "Pass numbers and booleans as native JSON values, or use typed params such as {\"kind\":\"bigint\",\"value\":\"13\"}.",
            ),
            Some(detail),
            Some(serde_json::json!({ "action": action })),
        );
    }
    let machine_code = if action.contains("read-only SQL query") {
        "sql_query_failed"
    } else if action.contains("SQL statement") {
        "sql_exec_failed"
    } else {
        "database_transaction_failed"
    };
    internal_error(
        machine_code,
        "SQL operation failed",
        Some("Check the SQL text, placeholder order, and database connectivity."),
        Some(detail),
        Some(serde_json::json!({ "action": action })),
    )
}

fn looks_like_sql_param_type_mismatch(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("operator does not exist")
        || lower.contains("could not determine data type")
        || lower.contains("invalid input syntax")
        || lower.contains("cannot cast")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{McpConfig, McpHttpMode, McpTransport};
    use sea_orm::{DbBackend, MockDatabase, Value};
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
