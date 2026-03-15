use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use rmcp::ErrorData as McpError;
use serde::Serialize;

use crate::tools::{
    generation_context::{
        CrudGenerationContext, CrudGenerationContextBuilder, FieldGenerationContext,
        FieldValueKind,
    },
    support::{io_error, resolve_output_dir, sync_mod_file, workspace_root},
    template_renderer::{EmbeddedTemplate, TemplateRenderer},
};

const APP_ROUTER_DIR: &str = "crates/app/src/router";
const APP_SERVICE_DIR: &str = "crates/app/src/service";
const MODEL_DTO_DIR: &str = "crates/model/src/dto";
const MODEL_VO_DIR: &str = "crates/model/src/vo";
const MODEL_ENTITY_DIR: &str = "crates/model/src/entity";

const ROUTER_TEMPLATE_NAME: &str = "backend/admin/router.rs.j2";
const SERVICE_TEMPLATE_NAME: &str = "backend/admin/service.rs.j2";
const DTO_TEMPLATE_NAME: &str = "backend/admin/dto.rs.j2";
const VO_TEMPLATE_NAME: &str = "backend/admin/vo.rs.j2";

const ADMIN_MODULE_TEMPLATES: [EmbeddedTemplate; 4] = [
    EmbeddedTemplate {
        name: ROUTER_TEMPLATE_NAME,
        source: include_str!("../../templates/backend/admin/router.rs.j2"),
    },
    EmbeddedTemplate {
        name: SERVICE_TEMPLATE_NAME,
        source: include_str!("../../templates/backend/admin/service.rs.j2"),
    },
    EmbeddedTemplate {
        name: DTO_TEMPLATE_NAME,
        source: include_str!("../../templates/backend/admin/dto.rs.j2"),
    },
    EmbeddedTemplate {
        name: VO_TEMPLATE_NAME,
        source: include_str!("../../templates/backend/admin/vo.rs.j2"),
    },
];

#[derive(Debug, Clone)]
pub struct AdminModuleGenerator {
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GenerateAdminModuleRequest {
    pub schema: crate::table_tools::schema::TableSchema,
    pub overwrite: bool,
    pub route_base: Option<String>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct GenerateAdminModuleResult {
    pub table: String,
    pub route_base: String,
    pub router_file: PathBuf,
    pub service_file: PathBuf,
    pub dto_file: PathBuf,
    pub vo_file: PathBuf,
    pub updated_mod_files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct GeneratedPaths {
    entity_file: PathBuf,
    router_file: PathBuf,
    service_file: PathBuf,
    dto_file: PathBuf,
    vo_file: PathBuf,
    router_mod_file: PathBuf,
    service_mod_file: PathBuf,
    dto_mod_file: PathBuf,
    vo_mod_file: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct AdminModuleTemplateContext {
    #[serde(flatten)]
    crud: CrudGenerationContext,
    imports: AdminModuleImportContext,
}

#[derive(Debug, Clone, Default, Serialize)]
struct AdminModuleImportContext {
    router: Vec<String>,
    service: Vec<String>,
    dto: Vec<String>,
    vo: Vec<String>,
}

impl AdminModuleGenerator {
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
        request: GenerateAdminModuleRequest,
    ) -> Result<GenerateAdminModuleResult, McpError> {
        let table = request.schema.table.clone();
        let paths = self.build_paths(&request.schema.table, request.output_dir.as_deref());
        let existing_targets = [
            &paths.router_file,
            &paths.service_file,
            &paths.dto_file,
            &paths.vo_file,
        ];
        for path in existing_targets {
            if tokio::fs::try_exists(path).await.map_err(|error| {
                io_error(format!("check generated file `{}`", path.display()), error)
            })? && !request.overwrite
            {
                return Err(McpError::invalid_params(
                    format!(
                        "generated file `{}` already exists; set overwrite=true to regenerate",
                        path.display()
                    ),
                    None,
                ));
            }
        }

        self.ensure_parent_dirs(&paths).await?;

        let entity_source = self.load_entity_source(&paths.entity_file).await?;
        let (context, router_source, service_source, dto_source, vo_source) = {
            let crud_context = CrudGenerationContextBuilder::build_from_entity_source(
                request.schema,
                request.route_base,
                &entity_source,
            )?;
            let render_context = build_admin_module_template_context(crud_context.clone());
            let renderer = TemplateRenderer::new(&ADMIN_MODULE_TEMPLATES)?;
            let router_source = renderer.render(ROUTER_TEMPLATE_NAME, &render_context)?;
            let service_source = renderer.render(SERVICE_TEMPLATE_NAME, &render_context)?;
            let dto_source = renderer.render(DTO_TEMPLATE_NAME, &render_context)?;
            let vo_source = renderer.render(VO_TEMPLATE_NAME, &render_context)?;
            (
                crud_context,
                router_source,
                service_source,
                dto_source,
                vo_source,
            )
        };

        tokio::fs::write(&paths.router_file, router_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write router file `{}`", paths.router_file.display()),
                    error,
                )
            })?;
        tokio::fs::write(&paths.service_file, service_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write service file `{}`", paths.service_file.display()),
                    error,
                )
            })?;
        tokio::fs::write(&paths.dto_file, dto_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write dto file `{}`", paths.dto_file.display()),
                    error,
                )
            })?;
        tokio::fs::write(&paths.vo_file, vo_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write vo file `{}`", paths.vo_file.display()),
                    error,
                )
            })?;

        sync_mod_file(&paths.router_mod_file, &table).await?;
        sync_mod_file(&paths.service_mod_file, &format!("{}_service", table)).await?;
        sync_mod_file(&paths.dto_mod_file, &table).await?;
        sync_mod_file(&paths.vo_mod_file, &table).await?;

        Ok(GenerateAdminModuleResult {
            table,
            route_base: context.table.route_base.clone(),
            router_file: paths.router_file,
            service_file: paths.service_file,
            dto_file: paths.dto_file,
            vo_file: paths.vo_file,
            updated_mod_files: vec![
                paths.router_mod_file,
                paths.service_mod_file,
                paths.dto_mod_file,
                paths.vo_mod_file,
            ],
        })
    }

    fn build_paths(&self, table: &str, output_dir: Option<&str>) -> GeneratedPaths {
        let output_root = match output_dir {
            Some(_) => resolve_output_dir(&self.workspace_root, output_dir, ""),
            None => self.workspace_root.clone(),
        };
        let (router_dir, service_dir, dto_dir, vo_dir) = match output_dir {
            Some(_) => (
                output_root.join("router"),
                output_root.join("service"),
                output_root.join("dto"),
                output_root.join("vo"),
            ),
            None => (
                output_root.join(APP_ROUTER_DIR),
                output_root.join(APP_SERVICE_DIR),
                output_root.join(MODEL_DTO_DIR),
                output_root.join(MODEL_VO_DIR),
            ),
        };

        GeneratedPaths {
            entity_file: self
                .workspace_root
                .join(MODEL_ENTITY_DIR)
                .join(format!("{table}.rs")),
            router_file: router_dir.join(format!("{table}.rs")),
            service_file: service_dir.join(format!("{table}_service.rs")),
            dto_file: dto_dir.join(format!("{table}.rs")),
            vo_file: vo_dir.join(format!("{table}.rs")),
            router_mod_file: router_dir.join("mod.rs"),
            service_mod_file: service_dir.join("mod.rs"),
            dto_mod_file: dto_dir.join("mod.rs"),
            vo_mod_file: vo_dir.join("mod.rs"),
        }
    }

    async fn ensure_parent_dirs(&self, paths: &GeneratedPaths) -> Result<(), McpError> {
        for directory in [
            paths.router_file.parent(),
            paths.service_file.parent(),
            paths.dto_file.parent(),
            paths.vo_file.parent(),
        ]
        .into_iter()
        .flatten()
        {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|error| {
                    io_error(
                        format!(
                            "create generator output directory `{}`",
                            directory.display()
                        ),
                        error,
                    )
                })?;
        }
        Ok(())
    }

    async fn load_entity_source(&self, entity_file: &Path) -> Result<String, McpError> {
        tokio::fs::read_to_string(entity_file)
            .await
            .map_err(|error| {
                io_error(
                    format!("read entity file `{}`", entity_file.display()),
                    error,
                )
            })
    }
}

fn build_admin_module_template_context(
    crud: CrudGenerationContext,
) -> AdminModuleTemplateContext {
    AdminModuleTemplateContext {
        imports: AdminModuleImportContext {
            router: vec![],
            service: vec![],
            dto: collect_backend_type_imports(
                crud.create_fields
                    .iter()
                    .chain(crud.update_fields.iter())
                    .chain(crud.query_fields.iter()),
            ),
            vo: collect_backend_type_imports(crud.read_fields.iter()),
        },
        crud,
    }
}

fn collect_backend_type_imports<'a, I>(fields: I) -> Vec<String>
where
    I: Iterator<Item = &'a FieldGenerationContext>,
{
    let mut imports = BTreeSet::new();
    for field in fields {
        if field.type_info.value_kind == FieldValueKind::Decimal {
            imports.insert("sea_orm::prelude::Decimal".to_string());
        }
    }
    imports.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use crate::tools::test_support::{SAMPLE_ROLE_ENTITY_SOURCE, sample_role_schema};

    use super::*;

    #[tokio::test]
    async fn generator_writes_compile_ready_skeleton_files() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-admin-module-generator-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        for dir in [
            root.join(APP_ROUTER_DIR),
            root.join(APP_SERVICE_DIR),
            root.join(MODEL_DTO_DIR),
            root.join(MODEL_VO_DIR),
            root.join(MODEL_ENTITY_DIR),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }

        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_role.rs"),
            SAMPLE_ROLE_ENTITY_SOURCE,
        )
        .unwrap();

        let generator = AdminModuleGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateAdminModuleRequest {
                schema: sample_role_schema(),
                overwrite: true,
                route_base: None,
                output_dir: None,
            })
            .await
            .unwrap();

        let router = std::fs::read_to_string(&result.router_file).unwrap();
        assert!(router.contains("pub async fn list("));
        assert!(router.contains("#[get_api(\"/role/list\")]"));

        let service = std::fs::read_to_string(&result.service_file).unwrap();
        assert!(service.contains("pub struct SysRoleService"));
        assert!(service.contains("pub async fn get_by_id"));

        let dto = std::fs::read_to_string(&result.dto_file).unwrap();
        assert!(dto.contains("pub struct CreateRoleDto"));
        assert!(dto.contains("impl From<CreateRoleDto> for sys_role::ActiveModel"));
        assert!(dto.contains("pub create_time_start: Option<chrono::NaiveDateTime>"));
        assert!(dto.contains("pub create_time_end: Option<chrono::NaiveDateTime>"));
        assert!(dto.contains("Column::CreateTime.gte(start)"));
        assert!(dto.contains("Column::CreateTime.lte(end)"));

        let vo = std::fs::read_to_string(&result.vo_file).unwrap();
        assert!(vo.contains("pub struct RoleVo"));
        assert!(vo.contains("pub create_time: chrono::NaiveDateTime"));

        let router_mod = std::fs::read_to_string(root.join(APP_ROUTER_DIR).join("mod.rs")).unwrap();
        assert!(router_mod.contains("pub mod sys_role;"));

        let service_mod =
            std::fs::read_to_string(root.join(APP_SERVICE_DIR).join("mod.rs")).unwrap();
        assert!(service_mod.contains("pub mod sys_role_service;"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_supports_custom_output_dir_without_touching_workspace_tree() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-admin-module-generator-output-dir-{}",
            std::process::id()
        ));
        let output_dir = root.join("generated/admin");
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_role.rs"),
            SAMPLE_ROLE_ENTITY_SOURCE,
        )
        .unwrap();

        let generator = AdminModuleGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateAdminModuleRequest {
                schema: sample_role_schema(),
                overwrite: true,
                route_base: None,
                output_dir: Some(output_dir.display().to_string()),
            })
            .await
            .unwrap();

        assert_eq!(result.router_file, output_dir.join("router/sys_role.rs"));
        assert_eq!(
            result.service_file,
            output_dir.join("service/sys_role_service.rs")
        );
        assert_eq!(result.dto_file, output_dir.join("dto/sys_role.rs"));
        assert_eq!(result.vo_file, output_dir.join("vo/sys_role.rs"));

        let router_mod = std::fs::read_to_string(output_dir.join("router/mod.rs")).unwrap();
        assert!(router_mod.contains("pub mod sys_role;"));

        let service_mod = std::fs::read_to_string(output_dir.join("service/mod.rs")).unwrap();
        assert!(service_mod.contains("pub mod sys_role_service;"));

        assert!(!root.join(APP_ROUTER_DIR).join("sys_role.rs").exists());
        assert!(
            !root
                .join(APP_SERVICE_DIR)
                .join("sys_role_service.rs")
                .exists()
        );
        assert!(!root.join(MODEL_DTO_DIR).join("sys_role.rs").exists());
        assert!(!root.join(MODEL_VO_DIR).join("sys_role.rs").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
