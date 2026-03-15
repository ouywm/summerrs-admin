use std::{
    collections::BTreeMap,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use rmcp::ErrorData as McpError;
use summer_domain::{
    dict::{DictBundleItemSpec, DictBundleSpec},
    menu::{MenuButtonSpec, MenuConfigSpec, MenuNodeSpec},
};
use tokio::process::Command;

use crate::{
    table_tools::schema::TableSchema,
    tools::{
        frontend_api_generator::{FrontendApiGenerator, GenerateFrontendApiRequest},
        frontend_page_generator::{
            FrontendFieldUiHint, FrontendPageGenerator, GenerateFrontendPageRequest,
        },
        frontend_target::FrontendTargetPreset,
        generation_context::{
            CrudGenerationContext, CrudGenerationContextBuilder, EnumOptionContext,
        },
        support::{io_error, workspace_root},
    },
};

const MODEL_ENTITY_DIR: &str = "crates/model/src/entity";

#[derive(Debug, Clone)]
pub struct FrontendBundleGenerator {
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GenerateFrontendBundleRequest {
    pub schema: TableSchema,
    pub overwrite: bool,
    pub route_base: Option<String>,
    /// Frontend bundle root directory.
    /// When set, generated files will be written to:
    /// - summer_mcp: <output_dir>/api, <output_dir>/api_type, <output_dir>/views/system/<route-base>/
    /// - art_design_pro: <output_dir>/src/api, <output_dir>/src/types/api, <output_dir>/src/views/system/<route-base>/
    pub output_dir: Option<String>,
    pub target_preset: FrontendTargetPreset,
    pub dict_bindings: BTreeMap<String, String>,
    pub field_hints: BTreeMap<String, FrontendFieldUiHint>,
    pub search_fields: Option<Vec<String>>,
    pub table_fields: Option<Vec<String>>,
    pub form_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct GenerateFrontendBundleResult {
    pub table: String,
    pub route_base: String,
    pub api_namespace: String,
    pub api_import_path: String,
    pub frontend_root_dir: PathBuf,
    pub api_file: PathBuf,
    pub api_type_file: PathBuf,
    pub page_dir: PathBuf,
    pub index_file: PathBuf,
    pub search_file: PathBuf,
    pub dialog_file: PathBuf,
    pub required_dict_types: Vec<String>,
    pub dict_bundle_drafts: Vec<DictBundleSpec>,
    pub menu_config_draft: MenuConfigSpec,
}

impl FrontendBundleGenerator {
    pub fn new() -> Result<Self, McpError> {
        Ok(Self {
            workspace_root: workspace_root()?,
        })
    }

    #[cfg(test)]
    fn with_workspace_root(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub async fn generate(
        &self,
        request: GenerateFrontendBundleRequest,
    ) -> Result<GenerateFrontendBundleResult, McpError> {
        let crud_context = self.load_crud_context(&request).await?;
        let effective_dict_bindings = build_effective_dict_bindings(&crud_context, &request);
        let dict_bundle_drafts = build_dict_bundle_drafts(&crud_context, &effective_dict_bindings);
        let menu_config_draft = build_menu_config_draft(&crud_context, request.target_preset);

        let layout = request
            .target_preset
            .resolve_bundle_layout(&self.workspace_root, request.output_dir.as_deref())?;
        let frontend_root_dir = layout.frontend_root_dir;
        let page_output_dir = match request.target_preset {
            FrontendTargetPreset::SummerMcp => Some(layout.view_system_dir.display().to_string()),
            FrontendTargetPreset::ArtDesignPro => request.output_dir.clone(),
        };

        let api_generator = FrontendApiGenerator::with_workspace_root(self.workspace_root.clone());
        let api_result = api_generator
            .generate(GenerateFrontendApiRequest {
                schema: request.schema.clone(),
                overwrite: request.overwrite,
                route_base: request.route_base.clone(),
                output_dir: request.output_dir.clone(),
                target_preset: request.target_preset,
            })
            .await?;

        let page_generator =
            FrontendPageGenerator::with_workspace_root(self.workspace_root.clone());
        let page_result = page_generator
            .generate(GenerateFrontendPageRequest {
                schema: request.schema,
                overwrite: request.overwrite,
                route_base: Some(api_result.route_base.clone()),
                output_dir: page_output_dir,
                target_preset: request.target_preset,
                api_import_path: None,
                api_namespace: None,
                api_list_item_type_name: None,
                api_detail_type_name: None,
                dict_bindings: effective_dict_bindings,
                field_hints: request.field_hints,
                search_fields: request.search_fields,
                table_fields: request.table_fields,
                form_fields: request.form_fields,
            })
            .await?;

        format_generated_frontend_bundle(
            request.target_preset,
            &frontend_root_dir,
            [
                api_result.api_file.as_path(),
                api_result.api_type_file.as_path(),
                page_result.index_file.as_path(),
                page_result.search_file.as_path(),
                page_result.dialog_file.as_path(),
            ],
        )
        .await?;

        Ok(GenerateFrontendBundleResult {
            table: api_result.table,
            route_base: api_result.route_base,
            api_namespace: page_result.api_namespace,
            api_import_path: page_result.api_import_path,
            frontend_root_dir,
            api_file: api_result.api_file,
            api_type_file: api_result.api_type_file,
            page_dir: page_result.page_dir,
            index_file: page_result.index_file,
            search_file: page_result.search_file,
            dialog_file: page_result.dialog_file,
            required_dict_types: page_result.required_dict_types,
            dict_bundle_drafts,
            menu_config_draft,
        })
    }

    async fn load_crud_context(
        &self,
        request: &GenerateFrontendBundleRequest,
    ) -> Result<CrudGenerationContext, McpError> {
        let entity_file = self
            .workspace_root
            .join(MODEL_ENTITY_DIR)
            .join(format!("{}.rs", request.schema.table));
        let entity_source = tokio::fs::read_to_string(&entity_file)
            .await
            .map_err(|error| {
                io_error(
                    format!("read entity file `{}`", entity_file.display()),
                    error,
                )
            })?;
        CrudGenerationContextBuilder::build_from_entity_source(
            request.schema.clone(),
            request.route_base.clone(),
            &entity_source,
        )
    }
}

async fn format_generated_frontend_bundle(
    target_preset: FrontendTargetPreset,
    frontend_root_dir: &Path,
    generated_files: [&Path; 5],
) -> Result<(), McpError> {
    if target_preset != FrontendTargetPreset::ArtDesignPro {
        return Ok(());
    }

    if !frontend_root_dir.join(".prettierrc").is_file() {
        return Ok(());
    }

    let local_prettier = prettier_bin(frontend_root_dir);
    let mut command = if local_prettier.is_file() {
        Command::new(local_prettier)
    } else {
        let mut command = Command::new("pnpm");
        command.arg("exec").arg("prettier");
        command
    };

    command.arg("--write");
    for path in generated_files {
        command.arg(path);
    }
    command.current_dir(frontend_root_dir);

    let output = match command.output().await {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            tracing::warn!(
                "skipping generated frontend formatting in `{}` because prettier is unavailable: {error}",
                frontend_root_dir.display()
            );
            return Ok(());
        }
        Err(error) => {
            return Err(io_error(
                format!(
                    "spawn prettier for generated frontend bundle in `{}`",
                    frontend_root_dir.display()
                ),
                error,
            ));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(McpError::internal_error(
        format!(
            "failed to format generated frontend bundle in `{}` with prettier (status {}): stdout:\n{}\nstderr:\n{}",
            frontend_root_dir.display(),
            output
                .status
                .code()
                .map_or_else(|| "terminated by signal".to_string(), |code| code.to_string()),
            stdout.trim(),
            stderr.trim()
        ),
        None,
    ))
}

fn prettier_bin(frontend_root_dir: &Path) -> PathBuf {
    let file_name = if cfg!(windows) {
        "prettier.cmd"
    } else {
        "prettier"
    };
    frontend_root_dir
        .join("node_modules")
        .join(".bin")
        .join(file_name)
}

fn build_effective_dict_bindings(
    crud_context: &CrudGenerationContext,
    request: &GenerateFrontendBundleRequest,
) -> BTreeMap<String, String> {
    let mut bindings = request.dict_bindings.clone();
    for field in &crud_context.fields {
        if bindings.contains_key(&field.name) || field.type_info.enum_options.is_empty() {
            continue;
        }
        bindings.insert(
            field.name.clone(),
            infer_dict_type_code(&crud_context.table.route_base, &field.name),
        );
    }
    bindings
}

fn build_dict_bundle_drafts(
    crud_context: &CrudGenerationContext,
    dict_bindings: &BTreeMap<String, String>,
) -> Vec<DictBundleSpec> {
    crud_context
        .fields
        .iter()
        .filter_map(|field| {
            let dict_type = dict_bindings.get(&field.name)?;
            if field.type_info.enum_options.is_empty() {
                return None;
            }
            Some(DictBundleSpec {
                dict_name: format!("{}{}", crud_context.table.subject_label, field.label),
                dict_type: dict_type.clone(),
                status: None,
                remark: None,
                items: field
                    .type_info
                    .enum_options
                    .iter()
                    .enumerate()
                    .map(|(idx, option)| DictBundleItemSpec {
                        dict_label: option.label.clone(),
                        dict_value: enum_option_value(option),
                        dict_sort: Some(idx as i32),
                        css_class: None,
                        list_class: None,
                        is_default: None,
                        status: None,
                        remark: None,
                    })
                    .collect(),
            })
        })
        .collect()
}

fn build_menu_config_draft(
    crud_context: &CrudGenerationContext,
    target_preset: FrontendTargetPreset,
) -> MenuConfigSpec {
    let route_base = crud_context.table.route_base.clone();
    let file_stem = crud_context.table.file_stem.clone();
    let resource_pascal = crud_context.names.resource_pascal.clone();
    let component = match target_preset {
        FrontendTargetPreset::SummerMcp => format!("system/{file_stem}/index"),
        FrontendTargetPreset::ArtDesignPro => format!("/system/{file_stem}"),
    };
    MenuConfigSpec {
        menus: vec![MenuNodeSpec {
            name: resource_pascal,
            path: route_base.clone(),
            component: Some(component),
            redirect: None,
            icon: None,
            title: crud_context.table.subject_label.clone(),
            link: None,
            is_iframe: None,
            is_hide: None,
            is_hide_tab: None,
            is_full_page: None,
            is_first_level: None,
            keep_alive: Some(true),
            fixed_tab: None,
            show_badge: None,
            show_text_badge: None,
            active_path: None,
            sort: Some(0),
            enabled: Some(true),
            buttons: vec![
                MenuButtonSpec {
                    auth_name: "新增".to_string(),
                    auth_mark: format!("{route_base}:add"),
                    sort: Some(1),
                    enabled: Some(true),
                },
                MenuButtonSpec {
                    auth_name: "编辑".to_string(),
                    auth_mark: format!("{route_base}:edit"),
                    sort: Some(2),
                    enabled: Some(true),
                },
                MenuButtonSpec {
                    auth_name: "删除".to_string(),
                    auth_mark: format!("{route_base}:delete"),
                    sort: Some(3),
                    enabled: Some(true),
                },
            ],
            children: vec![],
        }],
    }
}

fn infer_dict_type_code(route_base: &str, field_name: &str) -> String {
    format!("{route_base}_{field_name}")
}

fn enum_option_value(option: &EnumOptionContext) -> String {
    serde_json::from_str::<String>(&option.value_literal)
        .unwrap_or_else(|_| option.value_literal.clone())
}

#[cfg(test)]
mod tests {
    use crate::table_tools::schema::{
        TableCheckConstraintSchema, TableColumnSchema, TableIndexSchema,
    };

    use super::*;

    fn sample_user_schema() -> TableSchema {
        TableSchema {
            schema: "public".to_string(),
            table: "sys_user".to_string(),
            comment: Some("系统用户".to_string()),
            primary_key: vec!["id".to_string()],
            columns: vec![
                TableColumnSchema {
                    name: "id".to_string(),
                    pg_type: "bigint".to_string(),
                    nullable: false,
                    primary_key: true,
                    hidden_on_read: false,
                    writable_on_create: false,
                    writable_on_update: false,
                    default_value: Some("nextval(...)".to_string()),
                    comment: Some("用户ID".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "user_name".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: false,
                    default_value: None,
                    comment: Some("用户名".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "password".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: true,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("密码".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "avatar".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("头像URL".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "status".to_string(),
                    pg_type: "smallint".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: Some("1".to_string()),
                    comment: Some("状态".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "create_time".to_string(),
                    pg_type: "timestamp without time zone".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: false,
                    writable_on_update: false,
                    default_value: None,
                    comment: Some("创建时间".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
            ],
            indexes: vec![TableIndexSchema {
                name: "pk_sys_user".to_string(),
                columns: vec!["id".to_string()],
                unique: true,
                primary: true,
            }],
            foreign_keys: vec![],
            check_constraints: vec![TableCheckConstraintSchema {
                name: "status_check".to_string(),
                expression: "status >= 0".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn generator_writes_frontend_bundle_into_one_root_dir() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-bundle-generator-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join("crates/model/src/entity")).unwrap();
        std::fs::write(
            root.join("crates/model/src/entity/sys_user.rs"),
            r#"
/// 用户状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum UserStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled,
}

#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub user_name: String,
    pub password: String,
    pub avatar: String,
    pub status: UserStatus,
    pub create_time: DateTime,
}
"#,
        )
        .unwrap();

        let generator = FrontendBundleGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendBundleRequest {
                schema: sample_user_schema(),
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/frontend".to_string()),
                target_preset: FrontendTargetPreset::SummerMcp,
                dict_bindings: BTreeMap::from([("status".to_string(), "user_status".to_string())]),
                field_hints: BTreeMap::new(),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        assert_eq!(result.frontend_root_dir, root.join("generated/frontend"));
        assert_eq!(result.api_import_path, "@/api/user");
        assert_eq!(result.api_namespace, "User");
        assert_eq!(result.dict_bundle_drafts.len(), 1);
        assert_eq!(result.dict_bundle_drafts[0].dict_type, "user_status");
        assert_eq!(result.dict_bundle_drafts[0].items.len(), 2);
        assert_eq!(result.menu_config_draft.menus.len(), 1);
        assert_eq!(result.menu_config_draft.menus[0].path, "user");
        assert_eq!(
            result.menu_config_draft.menus[0].component.as_deref(),
            Some("system/user/index")
        );
        assert_eq!(
            result.menu_config_draft.menus[0].buttons[0].auth_mark,
            "user:add"
        );

        let api_file = std::fs::read_to_string(&result.api_file).unwrap();
        assert!(api_file.contains("fetchGetUserList"));

        let api_type_file = std::fs::read_to_string(&result.api_type_file).unwrap();
        assert!(api_type_file.contains("namespace User"));
        assert!(api_type_file.contains("interface UserVo"));

        let index_file = std::fs::read_to_string(&result.index_file).unwrap();
        assert!(index_file.contains("from '@/api/user'"));
        assert!(index_file.contains("getDictLabel('user_status'"));

        assert_eq!(result.required_dict_types, vec!["user_status".to_string()]);
        assert!(
            result
                .page_dir
                .starts_with(root.join("generated/frontend/views/system"))
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_supports_art_design_pro_bundle_layout() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-bundle-generator-adp-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join("crates/model/src/entity")).unwrap();
        std::fs::write(
            root.join("crates/model/src/entity/sys_user.rs"),
            r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub user_name: String,
    pub status: i16,
}
"#,
        )
        .unwrap();

        let generator = FrontendBundleGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendBundleRequest {
                schema: TableSchema {
                    schema: "public".to_string(),
                    table: "sys_user".to_string(),
                    comment: Some("系统用户".to_string()),
                    primary_key: vec!["id".to_string()],
                    columns: vec![
                        TableColumnSchema {
                            name: "id".to_string(),
                            pg_type: "bigint".to_string(),
                            nullable: false,
                            primary_key: true,
                            hidden_on_read: false,
                            writable_on_create: false,
                            writable_on_update: false,
                            default_value: None,
                            comment: Some("用户ID".to_string()),
                            is_identity: false,
                            is_generated: false,
                            enum_values: None,
                        },
                        TableColumnSchema {
                            name: "user_name".to_string(),
                            pg_type: "character varying".to_string(),
                            nullable: false,
                            primary_key: false,
                            hidden_on_read: false,
                            writable_on_create: true,
                            writable_on_update: true,
                            default_value: None,
                            comment: Some("用户名".to_string()),
                            is_identity: false,
                            is_generated: false,
                            enum_values: None,
                        },
                        TableColumnSchema {
                            name: "status".to_string(),
                            pg_type: "smallint".to_string(),
                            nullable: false,
                            primary_key: false,
                            hidden_on_read: false,
                            writable_on_create: true,
                            writable_on_update: true,
                            default_value: None,
                            comment: Some("状态".to_string()),
                            is_identity: false,
                            is_generated: false,
                            enum_values: None,
                        },
                    ],
                    indexes: vec![],
                    foreign_keys: vec![],
                    check_constraints: vec![],
                },
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/art-design-pro".to_string()),
                target_preset: FrontendTargetPreset::ArtDesignPro,
                dict_bindings: BTreeMap::new(),
                field_hints: BTreeMap::new(),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        assert_eq!(
            result.frontend_root_dir,
            root.join("generated/art-design-pro")
        );
        assert_eq!(
            result.api_file,
            root.join("generated/art-design-pro/src/api/user.ts")
        );
        assert_eq!(
            result.api_type_file,
            root.join("generated/art-design-pro/src/types/api/user.d.ts")
        );
        assert_eq!(
            result.page_dir,
            root.join("generated/art-design-pro/src/views/system/user")
        );
        assert_eq!(
            result.menu_config_draft.menus[0].component.as_deref(),
            Some("/system/user")
        );
        assert_eq!(
            result.menu_config_draft.menus[0].buttons[0].auth_mark,
            "user:add"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
