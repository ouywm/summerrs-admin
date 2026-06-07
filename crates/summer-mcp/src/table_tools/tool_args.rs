use std::collections::BTreeMap;

use rmcp::schemars;
use sea_orm::JsonValue;
use serde::Deserialize;
use summer_domain::{dict::DictBundleSpec, menu::MenuConfigSpec};
use summer_system_model::dto::{
    sys_dict::{
        CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto,
        UpdateDictDataDto, UpdateDictTypeDto,
    },
    sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto},
};

use crate::{
    table_tools::{
        query_builder::{TableFilterInput, TableSortInput},
        sql_scanner::SqlParamInput,
    },
    tools::{
        frontend_page_generator::{FrontendFieldUiHint, FrontendFieldUiMeta},
        frontend_target::FrontendTargetPreset,
    },
};

type JsonMap = BTreeMap<String, JsonValue>;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DescribeTableArgs {
    /// 需要查看的表名
    pub(crate) table: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableQueryArgs {
    /// 目标表名
    pub(crate) table: String,
    /// 需要返回的列，为空时返回所有可读列
    pub(crate) columns: Option<Vec<String>>,
    /// 过滤条件列表。支持结构化对象：
    /// [{"column":"id","op":"eq","value":1}]
    /// 也支持结构化分组：
    /// [{"or":[{"column":"status","op":"eq","value":1},{"column":"status","op":"eq","value":2}]}]
    /// 也支持简写字符串：
    /// ["id = 1", "role_name ilike admin", "status in [1,2,3]", "create_time between [\"2026-01-01\",\"2026-01-31\"]"]
    pub(crate) filters: Option<Vec<TableFilterInput>>,
    /// 排序条件，支持 [{"column":"id","direction":"desc"}] 或 ["id desc"]
    pub(crate) order_by: Option<Vec<TableSortInput>>,
    /// 返回条数，默认 20，最大 100
    pub(crate) limit: Option<u64>,
    /// 跳过条数，默认 0
    pub(crate) offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableGetArgs {
    /// 目标表名
    pub(crate) table: String,
    /// 主键对象，支持联合主键
    pub(crate) key: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableInsertArgs {
    /// 目标表名
    pub(crate) table: String,
    /// 新纪录字段值
    pub(crate) values: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableUpdateArgs {
    /// 目标表名
    pub(crate) table: String,
    /// 主键对象，支持联合主键
    pub(crate) key: JsonMap,
    /// 需要更新的字段值
    pub(crate) values: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableDeleteArgs {
    /// 目标表名
    pub(crate) table: String,
    /// 主键对象，支持联合主键
    pub(crate) key: JsonMap,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SqlQueryReadonlyArgs {
    /// 只读 SQL，当前仅允许单条 SELECT / WITH ... SELECT 语句
    pub(crate) sql: String,
    /// PostgreSQL 位置参数，对应 $1、$2 ...
    /// 推荐直接传 JSON 原生类型，例如 [13, true, "admin"]。
    /// 如果客户端只能传字符串，可显式传类型对象，例如：
    /// [{"kind":"bigint","value":"13"}]
    #[serde(default)]
    pub(crate) params: Vec<SqlParamInput>,
    /// 服务端返回行数上限，默认 200，最大 1000
    pub(crate) limit: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SqlExecArgs {
    /// 执行 SQL，允许单条 DDL / DML / 管理语句
    pub(crate) sql: String,
    /// PostgreSQL 位置参数，对应 $1、$2 ...
    /// 推荐直接传 JSON 原生类型，例如 [13, true, "admin"]。
    /// 如果客户端只能传字符串，可显式传类型对象，例如：
    /// [{"kind":"bigint","value":"13"}]
    #[serde(default)]
    pub(crate) params: Vec<SqlParamInput>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateEntityFromTableArgs {
    /// 需要生成 Entity 的表名
    pub(crate) table: String,
    /// 是否覆盖已有 entity 文件
    pub(crate) overwrite: Option<bool>,
    /// 输出目录，默认 crates/model/src/entity
    pub(crate) output_dir: Option<String>,
    /// 数据库连接串；未提供时优先使用 standalone 启动时的 --database-url，再回退到 DATABASE_URL
    pub(crate) database_url: Option<String>,
    /// 数据库 schema，默认 public
    pub(crate) database_schema: Option<String>,
    /// sea-orm-cli 可执行文件，默认 sea-orm-cli
    pub(crate) cli_bin: Option<String>,
    /// 可选：覆盖字段的 Rust 枚举名，例如 { "contact_gender": "ContactGender" }
    #[serde(default)]
    pub(crate) enum_name_overrides: BTreeMap<String, String>,
    /// 可选：覆盖枚举值到 Rust 变体名的映射，例如 { "status": { "1": "Enabled" } }
    #[serde(default)]
    pub(crate) variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateAdminModuleFromTableArgs {
    /// 需要生成后台模块骨架的表名
    pub(crate) table: String,
    /// 是否覆盖已有文件
    pub(crate) overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    pub(crate) route_base: Option<String>,
    /// 输出根目录；默认直接写入工作区既有 app/model 目录。
    /// 传入后会改写到：
    /// - <dir>/router
    /// - <dir>/service
    /// - <dir>/dto
    /// - <dir>/vo
    pub(crate) output_dir: Option<String>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    pub(crate) query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    pub(crate) create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    pub(crate) update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    pub(crate) list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    pub(crate) detail_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateFrontendApiFromTableArgs {
    /// 需要生成前端 api/类型声明 的表名
    pub(crate) table: String,
    /// 是否覆盖已有文件
    pub(crate) overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    pub(crate) route_base: Option<String>,
    /// 前端输出根目录。默认 target_preset=summer_mcp，生成 api 到 <dir>/api，类型声明到 <dir>/api_type。
    /// 当 target_preset=art_design_pro 时，生成 api 到 <dir>/src/api，类型声明到 <dir>/src/types/api。
    pub(crate) output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    pub(crate) target_preset: Option<FrontendTargetPreset>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    pub(crate) query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    pub(crate) create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    pub(crate) update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    pub(crate) list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    pub(crate) detail_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateFrontendBundleFromTableArgs {
    /// 需要一次生成 frontend api/类型声明/page 的表名
    pub(crate) table: String,
    /// 是否覆盖已有文件
    pub(crate) overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    pub(crate) route_base: Option<String>,
    /// 前端输出根目录；默认 target_preset=summer_mcp。
    /// 生成结果会写到：
    /// - summer_mcp: <dir>/api, <dir>/api_type, <dir>/views/system/<route-base-kebab>/
    /// - art_design_pro: <dir>/src/api, <dir>/src/types/api, <dir>/src/views/system/<route-base-kebab>/
    pub(crate) output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    pub(crate) target_preset: Option<FrontendTargetPreset>,
    /// 字典绑定，key 为字段名，value 为字典类型编码，例如 { "status": "user_status" }
    #[serde(default)]
    pub(crate) dict_bindings: BTreeMap<String, String>,
    /// 显式字段 UI 提示，供 AI 覆盖默认推断，例如将 avatar 指定为 image/avatar 上传、强制隐藏搜索项等
    #[serde(default)]
    pub(crate) field_hints: BTreeMap<String, FrontendFieldUiHint>,
    /// 结构化字段 UI 元数据。优先级高于 field_hints / dict_bindings，可精确控制搜索、表单、表格组件与可见性
    #[serde(default)]
    pub(crate) field_ui_meta: BTreeMap<String, FrontendFieldUiMeta>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    pub(crate) query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    pub(crate) create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    pub(crate) update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    pub(crate) list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    pub(crate) detail_fields: Option<Vec<String>>,
    /// 显式指定搜索区字段；未传时会按字段语义自动排序并选出所有适合搜索的字段
    pub(crate) search_fields: Option<Vec<String>>,
    /// 显式指定表格列字段，默认自动选择所有可读字段
    pub(crate) table_fields: Option<Vec<String>>,
    /// 显式指定弹窗表单字段，默认自动选择 create/update 字段并做并集
    pub(crate) form_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateFrontendPageFromTableArgs {
    /// 需要生成前端页面骨架的表名
    pub(crate) table: String,
    /// 是否覆盖已有文件
    pub(crate) overwrite: Option<bool>,
    /// 路由基础路径，默认自动从表名推导，例如 sys_role -> role
    pub(crate) route_base: Option<String>,
    /// 页面输出目录。默认 target_preset=summer_mcp，最终写到 <output_dir>/<route-base-kebab>/。
    /// 当 target_preset=art_design_pro 时，output_dir 应指向前端项目根目录，最终写到 <output_dir>/src/views/system/<route-base-kebab>/。
    pub(crate) output_dir: Option<String>,
    /// 前端输出 preset，默认 summer_mcp；art_design_pro 需要 output_dir 指向前端项目根目录
    pub(crate) target_preset: Option<FrontendTargetPreset>,
    /// 高级：覆盖页面导入的前端 API 模块路径；默认直接使用生成器产出的 @/api/<route-base-kebab>
    pub(crate) api_import_path: Option<String>,
    /// 高级：覆盖全局 API TypeScript namespace；默认直接使用生成器推导值，例如 Role
    pub(crate) api_namespace: Option<String>,
    /// 高级：仅在适配现有手写业务 API 类型时使用；默认直接使用生成器产出的 <Resource>Vo
    pub(crate) api_list_item_type_name: Option<String>,
    /// 高级：仅在适配现有手写业务 API 类型时使用；默认直接使用生成器产出的 <Resource>DetailVo
    pub(crate) api_detail_type_name: Option<String>,
    /// 字典绑定，key 为字段名，value 为字典类型编码，例如 { "status": "user_status" }
    #[serde(default)]
    pub(crate) dict_bindings: BTreeMap<String, String>,
    /// 显式字段 UI 提示，供 AI 覆盖默认推断，例如将 avatar 指定为 image/avatar 上传、强制隐藏搜索项等
    #[serde(default)]
    pub(crate) field_hints: BTreeMap<String, FrontendFieldUiHint>,
    /// 结构化字段 UI 元数据。优先级高于 field_hints / dict_bindings，可精确控制搜索、表单、表格组件与可见性
    #[serde(default)]
    pub(crate) field_ui_meta: BTreeMap<String, FrontendFieldUiMeta>,
    /// 显式后端/前端查询契约；未传时使用生成器默认查询字段选择
    pub(crate) query_fields: Option<Vec<String>>,
    /// 显式创建参数契约；未传时使用可写创建字段
    pub(crate) create_fields: Option<Vec<String>>,
    /// 显式更新参数契约；未传时使用可写更新字段
    pub(crate) update_fields: Option<Vec<String>>,
    /// 显式列表返回契约；未传时使用可读字段
    pub(crate) list_fields: Option<Vec<String>>,
    /// 显式详情返回契约；未传时使用可读字段
    pub(crate) detail_fields: Option<Vec<String>>,
    /// 显式指定搜索区字段；未传时会按字段语义自动排序并选出所有适合搜索的字段
    pub(crate) search_fields: Option<Vec<String>>,
    /// 显式指定表格列字段，默认自动选择所有可读字段
    pub(crate) table_fields: Option<Vec<String>>,
    /// 显式指定弹窗表单字段，默认自动选择 create/update 字段并做并集
    pub(crate) form_fields: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum UpgradeEntityEnumsFromTableArgs {
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
pub(crate) enum MenuToolArgs {
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
pub(crate) enum DictToolArgs {
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
