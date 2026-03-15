use std::{collections::BTreeMap, path::PathBuf};

use common::error::ApiErrors;
use model::{
    dto::sys_dict::{
        CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto,
        UpdateDictDataDto, UpdateDictTypeDto,
    },
    dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto},
    vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo},
    vo::sys_menu::{MenuTreeVo, MenuVo},
};
use rmcp::{
    ErrorData as McpError, Json, handler::server::wrapper::Parameters, schemars, tool, tool_router,
};
use sea_orm::{
    AccessMode, ConnectionTrait, DbBackend, FromQueryResult, JsonValue, SelectModel, SelectorRaw,
    Statement, TransactionError, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use summer_domain::{
    dict::{DictBundleSpec, DictBundleSyncResult},
    menu::{MenuConfigSpec, MenuConfigSyncResult},
};

use crate::{
    server::AdminMcpServer,
    table_tools::{
        query_builder::{
            TableFilter, TableSortInput, build_filters_clause, build_insert_assignments,
            build_key_clause, build_order_clause, build_update_assignments,
        },
        schema::{
            TableSchema, db_error, describe_table, ensure_valid_identifier, list_tables,
            quote_identifier, readable_select_list,
        },
        sql_scanner::{convert_sql_params, normalize_exec_sql, normalize_readonly_sql},
    },
    tools::{
        admin_module_generator::{AdminModuleGenerator, GenerateAdminModuleRequest},
        entity_generator::{EntityGenerator, GenerateEntityRequest},
        frontend_api_generator::{FrontendApiGenerator, GenerateFrontendApiRequest},
        frontend_bundle_generator::{FrontendBundleGenerator, GenerateFrontendBundleRequest},
        frontend_page_generator::{
            FrontendFieldUiHint, FrontendPageGenerator, GenerateFrontendPageRequest,
        },
        frontend_target::FrontendTargetPreset,
        support::{
            error_chain_message, resolve_output_dir, sanitize_file_stem, workspace_root,
            write_pretty_json_file,
        },
    },
};

const DEFAULT_LIST_LIMIT: u64 = 20;
const MAX_LIST_LIMIT: u64 = 100;
const DEFAULT_SQL_QUERY_LIMIT: u64 = 200;
const MAX_SQL_QUERY_LIMIT: u64 = 1_000;
const READONLY_SQL_SUBQUERY_ALIAS: &str = "__summer_mcp_readonly";

type JsonMap = BTreeMap<String, JsonValue>;

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
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct GenerateFrontendApiFromTableResult {
    pub table: String,
    pub route_base: String,
    pub namespace: String,
    pub api_file: String,
    pub api_type_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct GenerateFrontendPageFromTableResult {
    pub table: String,
    pub route_base: String,
    pub api_import_path: String,
    pub api_namespace: String,
    pub page_dir: String,
    pub index_file: String,
    pub search_file: String,
    pub dialog_file: String,
    pub required_dict_types: Vec<String>,
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
    pub index_file: String,
    pub search_file: String,
    pub dialog_file: String,
    pub required_dict_types: Vec<String>,
    pub dict_bundle_drafts: Vec<DictBundleSpec>,
    pub menu_config_draft: MenuConfigSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ExportArtifactsResult {
    pub output_dir: String,
    pub spec_file: String,
    pub plan_file: String,
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
    /// 过滤条件列表
    filters: Option<Vec<TableFilter>>,
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
    #[serde(default)]
    params: Vec<JsonValue>,
    /// 服务端返回行数上限，默认 200，最大 1000
    limit: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SqlExecArgs {
    /// 执行 SQL，允许单条 DDL / DML / 管理语句
    sql: String,
    /// PostgreSQL 位置参数，对应 $1、$2 ...
    #[serde(default)]
    params: Vec<JsonValue>,
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
    /// 显式指定搜索区字段；未传时会按字段语义自动排序并选出所有适合搜索的字段
    search_fields: Option<Vec<String>>,
    /// 显式指定表格列字段，默认自动选择所有可读字段
    table_fields: Option<Vec<String>>,
    /// 显式指定弹窗表单字段，默认自动选择 create/update 字段并做并集
    form_fields: Option<Vec<String>>,
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

#[tool_router(router = tool_router, vis = "pub(crate)")]
impl AdminMcpServer {
    #[tool(description = "List runtime-discovered database tables exposed by this MCP server")]
    async fn schema_list_tables(&self) -> Result<Json<ListTablesResult>, McpError> {
        let tables = list_tables(self.db()).await?;
        Ok(Json(ListTablesResult {
            schema: "public".to_string(),
            tables,
        }))
    }

    #[tool(
        description = "Describe a database table at runtime, including primary keys and readable/writable columns"
    )]
    async fn schema_describe_table(
        &self,
        Parameters(args): Parameters<DescribeTableArgs>,
    ) -> Result<Json<TableSchema>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
        Ok(Json(schema))
    }

    #[tool(
        description = "Generate or regenerate one SeaORM entity file from a live database table via sea-orm-cli and sync crates/model/src/entity/mod.rs"
    )]
    async fn generate_entity_from_table(
        &self,
        Parameters(args): Parameters<GenerateEntityFromTableArgs>,
    ) -> Result<Json<GenerateEntityFromTableResult>, McpError> {
        ensure_valid_identifier(&args.table, "table")?;

        let generator = EntityGenerator::new(self.default_database_url().map(ToOwned::to_owned))?;
        let result = generator
            .generate(GenerateEntityRequest {
                table: args.table,
                overwrite: args.overwrite.unwrap_or(false),
                output_dir: args.output_dir,
                database_url: args.database_url,
                database_schema: args.database_schema,
                cli_bin: args.cli_bin,
            })
            .await?;

        Ok(Json(GenerateEntityFromTableResult {
            table: result.table,
            entity_file: result.entity_file.display().to_string(),
            mod_file: result.mod_file.display().to_string(),
            overwritten: result.overwritten,
            database_schema: result.database_schema,
            cli_bin: result.cli_bin,
        }))
    }

    #[tool(
        description = "Generate a compile-ready admin CRUD skeleton for one single-primary-key table, including router/service/dto/vo modules. Pass output_dir to write into a temp directory instead of the workspace."
    )]
    async fn generate_admin_module_from_table(
        &self,
        Parameters(args): Parameters<GenerateAdminModuleFromTableArgs>,
    ) -> Result<Json<GenerateAdminModuleFromTableResult>, McpError> {
        ensure_valid_identifier(&args.table, "table")?;
        if let Some(route_base) = &args.route_base {
            ensure_valid_identifier(route_base, "route_base")?;
        }

        let schema = describe_table(self.db(), &args.table).await?;
        let generator = AdminModuleGenerator::new()?;
        let result = generator
            .generate(GenerateAdminModuleRequest {
                schema,
                overwrite: args.overwrite.unwrap_or(false),
                route_base: args.route_base,
                output_dir: args.output_dir,
            })
            .await?;

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
        }))
    }

    #[tool(
        description = "Generate frontend API wrappers and global TypeScript declarations for one table. Default target_preset=summer_mcp writes into crates/app/frontend-routes/api and api_type; art_design_pro writes into src/api and src/types/api."
    )]
    async fn generate_frontend_api_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendApiFromTableArgs>,
    ) -> Result<Json<GenerateFrontendApiFromTableResult>, McpError> {
        ensure_valid_identifier(&args.table, "table")?;
        if let Some(route_base) = &args.route_base {
            ensure_valid_identifier(route_base, "route_base")?;
        }

        let schema = describe_table(self.db(), &args.table).await?;
        let generator = FrontendApiGenerator::new()?;
        let result = generator
            .generate(GenerateFrontendApiRequest {
                schema,
                overwrite: args.overwrite.unwrap_or(false),
                route_base: args.route_base,
                output_dir: args.output_dir,
                target_preset: args.target_preset.unwrap_or_default(),
            })
            .await?;

        Ok(Json(GenerateFrontendApiFromTableResult {
            table: result.table,
            route_base: result.route_base,
            namespace: result.namespace,
            api_file: result.api_file.display().to_string(),
            api_type_file: result.api_type_file.display().to_string(),
        }))
    }

    #[tool(
        description = "Generate frontend api/type/page in one shot for one table. By default target_preset=summer_mcp writes a self-consistent generated bundle into crates/app/frontend-routes; art_design_pro writes into src/api, src/types/api, and src/views/system. The tool auto-infers enum-backed dict bindings and returns menu/dict drafts that AI can pass to menu_tool and dict_tool for review or apply."
    )]
    async fn generate_frontend_bundle_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendBundleFromTableArgs>,
    ) -> Result<Json<GenerateFrontendBundleFromTableResult>, McpError> {
        ensure_valid_identifier(&args.table, "table")?;
        if let Some(route_base) = &args.route_base {
            ensure_valid_identifier(route_base, "route_base")?;
        }

        let schema = describe_table(self.db(), &args.table).await?;
        let generator = FrontendBundleGenerator::new()?;
        let result = generator
            .generate(GenerateFrontendBundleRequest {
                schema,
                overwrite: args.overwrite.unwrap_or(false),
                route_base: args.route_base,
                output_dir: args.output_dir,
                target_preset: args.target_preset.unwrap_or_default(),
                dict_bindings: args.dict_bindings,
                field_hints: args.field_hints,
                search_fields: args.search_fields,
                table_fields: args.table_fields,
                form_fields: args.form_fields,
            })
            .await?;

        Ok(Json(GenerateFrontendBundleFromTableResult {
            table: result.table,
            route_base: result.route_base,
            api_namespace: result.api_namespace,
            api_import_path: result.api_import_path,
            frontend_root_dir: result.frontend_root_dir.display().to_string(),
            api_file: result.api_file.display().to_string(),
            api_type_file: result.api_type_file.display().to_string(),
            page_dir: result.page_dir.display().to_string(),
            index_file: result.index_file.display().to_string(),
            search_file: result.search_file.display().to_string(),
            dialog_file: result.dialog_file.display().to_string(),
            required_dict_types: result.required_dict_types,
            dict_bundle_drafts: result.dict_bundle_drafts,
            menu_config_draft: result.menu_config_draft,
        }))
    }

    #[tool(
        description = "Generate an Art Design Pro style frontend CRUD page for one table. By default the page targets the generated frontend api/type contract for the same table; advanced overrides are only needed when adapting an existing handwritten business API contract."
    )]
    async fn generate_frontend_page_from_table(
        &self,
        Parameters(args): Parameters<GenerateFrontendPageFromTableArgs>,
    ) -> Result<Json<GenerateFrontendPageFromTableResult>, McpError> {
        ensure_valid_identifier(&args.table, "table")?;
        if let Some(route_base) = &args.route_base {
            ensure_valid_identifier(route_base, "route_base")?;
        }

        let schema = describe_table(self.db(), &args.table).await?;
        let generator = FrontendPageGenerator::new()?;
        let result = generator
            .generate(GenerateFrontendPageRequest {
                schema,
                overwrite: args.overwrite.unwrap_or(false),
                route_base: args.route_base,
                output_dir: args.output_dir,
                target_preset: args.target_preset.unwrap_or_default(),
                api_import_path: args.api_import_path,
                api_namespace: args.api_namespace,
                api_list_item_type_name: args.api_list_item_type_name,
                api_detail_type_name: args.api_detail_type_name,
                dict_bindings: args.dict_bindings,
                field_hints: args.field_hints,
                search_fields: args.search_fields,
                table_fields: args.table_fields,
                form_fields: args.form_fields,
            })
            .await?;

        Ok(Json(GenerateFrontendPageFromTableResult {
            table: result.table,
            route_base: result.route_base,
            api_import_path: result.api_import_path,
            api_namespace: result.api_namespace,
            page_dir: result.page_dir.display().to_string(),
            index_file: result.index_file.display().to_string(),
            search_file: result.search_file.display().to_string(),
            dialog_file: result.dialog_file.display().to_string(),
            required_dict_types: result.required_dict_types,
        }))
    }

    #[tool(
        description = "Read or mutate menu business data with domain rules instead of raw SQL. Required field: `action`. Supported actions: `list_tree`, `get_user_tree`, `plan_config`, `export_config`, `apply_config`, `create_menu`, `create_button`, `update_menu`, `update_button`, `delete_node`."
    )]
    async fn menu_tool(
        &self,
        Parameters(args): Parameters<MenuToolArgs>,
    ) -> Result<Json<MenuToolResponse>, McpError> {
        let domain = self.menu_domain();
        let result = match args {
            MenuToolArgs::ListTree => MenuToolResult::Tree {
                items: domain.list_menus().await.map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::GetUserTree { user_id } => MenuToolResult::Tree {
                items: domain
                    .get_menu_tree_for_user_id(user_id)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::PlanConfig { config } => MenuToolResult::ConfigSync {
                sync: domain
                    .plan_menu_config(&config)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::ExportConfig { config, output_dir } => {
                let sync = domain
                    .plan_menu_config(&config)
                    .await
                    .map_err(api_error_to_mcp)?;
                let export = export_menu_config_artifacts(&config, &sync, &output_dir).await?;
                MenuToolResult::ConfigExport { export, sync }
            }
            MenuToolArgs::ApplyConfig { config } => MenuToolResult::ConfigSync {
                sync: domain
                    .apply_menu_config(config)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::CreateMenu { data } => MenuToolResult::Menu {
                item: domain.create_menu(data).await.map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::CreateButton { data } => MenuToolResult::Menu {
                item: domain.create_button(data).await.map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::UpdateMenu { id, data } => MenuToolResult::Menu {
                item: domain
                    .update_menu(id, data)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::UpdateButton { id, data } => MenuToolResult::Menu {
                item: domain
                    .update_button(id, data)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            MenuToolArgs::DeleteNode { id } => MenuToolResult::Deleted {
                id: domain.delete_menu(id).await.map_err(api_error_to_mcp)?,
            },
        };
        Ok(Json(MenuToolResponse { result }))
    }

    #[tool(
        description = "Read or mutate dictionary business data with domain rules instead of raw SQL. Required field: `action`. Supported actions: `list_types`, `list_data`, `get_by_type`, `get_all_enabled`, `plan_bundle`, `export_bundle`, `apply_bundle`, `create_type`, `update_type`, `delete_type`, `create_data`, `update_data`, `delete_data`."
    )]
    async fn dict_tool(
        &self,
        Parameters(args): Parameters<DictToolArgs>,
    ) -> Result<Json<DictToolResponse>, McpError> {
        let domain = self.dict_domain();
        let result = match args {
            DictToolArgs::ListTypes { query } => DictToolResult::TypeList {
                items: domain
                    .list_dict_types(query.unwrap_or_else(empty_dict_type_query))
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            DictToolArgs::ListData { query } => DictToolResult::DataList {
                items: domain
                    .list_dict_data(query.unwrap_or_else(empty_dict_data_query))
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            DictToolArgs::GetByType { dict_type } => DictToolResult::SimpleDataList {
                items: domain
                    .get_dict_data_by_type(&dict_type)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            DictToolArgs::GetAllEnabled => DictToolResult::AllData {
                data: domain.get_all_dict_data().await.map_err(api_error_to_mcp)?,
            },
            DictToolArgs::PlanBundle { bundle } => DictToolResult::BundleSync {
                sync: domain
                    .plan_dict_bundle(&bundle)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            DictToolArgs::ExportBundle { bundle, output_dir } => {
                let sync = domain
                    .plan_dict_bundle(&bundle)
                    .await
                    .map_err(api_error_to_mcp)?;
                let export = export_dict_bundle_artifacts(&bundle, &sync, &output_dir).await?;
                DictToolResult::BundleExport { export, sync }
            }
            DictToolArgs::ApplyBundle { operator, bundle } => {
                let operator = operator_name(operator);
                DictToolResult::BundleSync {
                    sync: domain
                        .apply_dict_bundle(bundle, &operator)
                        .await
                        .map_err(api_error_to_mcp)?,
                }
            }
            DictToolArgs::CreateType { operator, data } => {
                let operator = operator_name(operator);
                DictToolResult::Type {
                    item: domain
                        .create_dict_type(data, &operator)
                        .await
                        .map_err(api_error_to_mcp)?,
                }
            }
            DictToolArgs::UpdateType { id, operator, data } => {
                let operator = operator_name(operator);
                DictToolResult::Type {
                    item: domain
                        .update_dict_type(id, data, &operator)
                        .await
                        .map_err(api_error_to_mcp)?,
                }
            }
            DictToolArgs::DeleteType { id } => DictToolResult::Deleted {
                id: domain
                    .delete_dict_type(id)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
            DictToolArgs::CreateData { operator, data } => {
                let operator = operator_name(operator);
                DictToolResult::Data {
                    item: domain
                        .create_dict_data(data, &operator)
                        .await
                        .map_err(api_error_to_mcp)?,
                }
            }
            DictToolArgs::UpdateData { id, operator, data } => {
                let operator = operator_name(operator);
                DictToolResult::Data {
                    item: domain
                        .update_dict_data(id, data, &operator)
                        .await
                        .map_err(api_error_to_mcp)?,
                }
            }
            DictToolArgs::DeleteData { id } => DictToolResult::Deleted {
                id: domain
                    .delete_dict_data(id)
                    .await
                    .map_err(api_error_to_mcp)?,
            },
        };
        Ok(Json(DictToolResponse { result }))
    }

    #[tool(
        name = "sql_query_readonly",
        description = "Execute one read-only SQL query for complex reads that cannot be expressed by table_query"
    )]
    async fn sql_query_readonly_tool(
        &self,
        Parameters(args): Parameters<SqlQueryReadonlyArgs>,
    ) -> Result<Json<SqlQueryReadonlyResult>, McpError> {
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
                        .map_err(|error| db_error("execute read-only SQL query", error))
                    })
                },
                None,
                Some(AccessMode::ReadOnly),
            )
            .await
            .map_err(|error| match error {
                TransactionError::Connection(error) => {
                    db_error("start read-only SQL transaction", error)
                }
                TransactionError::Transaction(error) => error,
            })?;

        let result = SqlQueryReadonlyResult {
            row_count: rows.len() as u64,
            rows,
            limit,
        };
        Ok(Json(result))
    }

    #[tool(
        name = "sql_exec",
        description = "Execute one SQL statement for DDL or data modification. Use sql_query_readonly for reads."
    )]
    async fn sql_exec_tool(
        &self,
        Parameters(args): Parameters<SqlExecArgs>,
    ) -> Result<Json<SqlExecResult>, McpError> {
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
                        .map_err(|error| db_error("execute SQL statement", error))?;
                    Ok(result.rows_affected())
                })
            })
            .await
            .map_err(|error| match error {
                TransactionError::Connection(error) => {
                    db_error("start sql_exec transaction", error)
                }
                TransactionError::Transaction(error) => error,
            })?;

        Ok(Json(SqlExecResult { rows_affected }))
    }

    #[tool(description = "Fetch one row from a table by primary key")]
    async fn table_get(
        &self,
        Parameters(args): Parameters<TableGetArgs>,
    ) -> Result<Json<TableLookupResult>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
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

        let item = SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
            .one(self.db())
            .await
            .map_err(|error| db_error(format!("query row from `{}`", schema.table), error))?;

        Ok(Json(TableLookupResult {
            schema: schema.schema,
            table: schema.table,
            found: item.is_some(),
            item,
        }))
    }

    #[tool(
        description = "Query rows from a table with runtime schema validation, filters, sorting, and pagination"
    )]
    async fn table_query(
        &self,
        Parameters(args): Parameters<TableQueryArgs>,
    ) -> Result<Json<TableListResult>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
        let select_list = readable_select_list(&schema, args.columns.as_deref())?;
        let window = ListWindow::from_args(args.limit, args.offset);

        let (where_clause, count_params) = build_filters_clause(&schema, args.filters.as_deref())?;
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
                .map_err(|error| db_error(format!("query rows from `{}`", schema.table), error))?;

        Ok(Json(TableListResult {
            schema: schema.schema,
            table: schema.table,
            items,
            total,
            limit: window.limit,
            offset: window.offset,
        }))
    }

    #[tool(description = "Insert one row into a table and return the created row")]
    async fn table_insert(
        &self,
        Parameters(args): Parameters<TableInsertArgs>,
    ) -> Result<Json<TableMutationResult>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
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

        let item = SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
            .one(self.db())
            .await
            .map_err(|error| db_error(format!("insert row into `{}`", schema.table), error))?;

        Ok(Json(TableMutationResult {
            schema: schema.schema,
            table: schema.table,
            found: item.is_some(),
            changed: item.is_some(),
            item,
        }))
    }

    #[tool(description = "Update one row in a table by primary key and return the latest row")]
    async fn table_update(
        &self,
        Parameters(args): Parameters<TableUpdateArgs>,
    ) -> Result<Json<TableMutationResult>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
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

        let item = SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
            .one(self.db())
            .await
            .map_err(|error| db_error(format!("update row in `{}`", schema.table), error))?;

        let (found, changed) = if item.is_some() {
            (true, true)
        } else {
            // UPDATE RETURNING returned nothing — check whether the row exists at all
            // so we can distinguish "not found" from "found but not changed".
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
    }

    #[tool(description = "Delete one row from a table by primary key")]
    async fn table_delete(
        &self,
        Parameters(args): Parameters<TableDeleteArgs>,
    ) -> Result<Json<TableDeleteResult>, McpError> {
        let schema = describe_table(self.db(), &args.table).await?;
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

        let deleted = SelectorRaw::<SelectModel<JsonValue>>::from_statement::<JsonValue>(statement)
            .one(self.db())
            .await
            .map_err(|error| db_error(format!("delete row from `{}`", schema.table), error))?
            .is_some();

        Ok(Json(TableDeleteResult {
            schema: schema.schema,
            table: schema.table,
            found: deleted,
            deleted,
            rows_affected: u64::from(deleted),
        }))
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
    })
}

fn resolve_export_output_dir(output_dir: &str) -> Result<PathBuf, McpError> {
    if output_dir.trim().is_empty() {
        return Err(McpError::invalid_params("output_dir cannot be empty", None));
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
        ApiErrors::Internal(error) => {
            McpError::internal_error(error_chain_message(error.as_ref()), None)
        }
        ApiErrors::ServiceUnavailable(message) => McpError::internal_error(message, None),
        ApiErrors::BadRequest(message)
        | ApiErrors::Unauthorized(message)
        | ApiErrors::Forbidden(message)
        | ApiErrors::NotFound(message)
        | ApiErrors::Conflict(message)
        | ApiErrors::IncompleteUpload(message)
        | ApiErrors::ValidationFailed(message)
        | ApiErrors::TooManyRequests(message) => McpError::invalid_params(message, None),
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

#[cfg(test)]
mod tests {
    use super::*;

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
                "sql_exec",
                "sql_query_readonly",
                "table_delete",
                "table_get",
                "table_insert",
                "table_query",
                "table_update",
            ]
        );
    }
}
