use std::path::{Path, PathBuf};

use rmcp::ErrorData as McpError;

use crate::tools::{
    frontend_target::FrontendTargetPreset,
    generation_context::CrudGenerationContextBuilder,
    support::{default_route_base, io_error, workspace_root},
    template_renderer::{EmbeddedTemplate, TemplateRenderer},
};

const MODEL_ENTITY_DIR: &str = "crates/model/src/entity";

const FRONTEND_API_TEMPLATE_NAME: &str = "frontend/api/api.ts.j2";
const FRONTEND_API_TYPE_TEMPLATE_NAME: &str = "frontend/api/api_type.d.ts.j2";

const FRONTEND_API_TEMPLATES: [EmbeddedTemplate; 2] = [
    EmbeddedTemplate {
        name: FRONTEND_API_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/api/api.ts.j2"),
    },
    EmbeddedTemplate {
        name: FRONTEND_API_TYPE_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/api/api_type.d.ts.j2"),
    },
];

#[derive(Debug, Clone)]
pub struct FrontendApiGenerator {
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GenerateFrontendApiRequest {
    pub schema: crate::table_tools::schema::TableSchema,
    pub overwrite: bool,
    pub route_base: Option<String>,
    pub output_dir: Option<String>,
    pub target_preset: FrontendTargetPreset,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct GenerateFrontendApiResult {
    pub table: String,
    pub route_base: String,
    pub namespace: String,
    pub api_file: PathBuf,
    pub api_type_file: PathBuf,
}

#[derive(Debug, Clone)]
struct GeneratedPaths {
    entity_file: PathBuf,
    api_file: PathBuf,
    api_type_file: PathBuf,
}

impl FrontendApiGenerator {
    pub fn new() -> Result<Self, McpError> {
        Ok(Self {
            workspace_root: workspace_root()?,
        })
    }

    pub(crate) fn with_workspace_root(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub async fn generate(
        &self,
        request: GenerateFrontendApiRequest,
    ) -> Result<GenerateFrontendApiResult, McpError> {
        let paths = self.build_paths(
            &request.schema.table,
            request.route_base.as_deref(),
            request.output_dir.as_deref(),
            request.target_preset,
        )?;
        for path in [&paths.api_file, &paths.api_type_file] {
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
        let (context, api_source, api_type_source) = {
            let context = CrudGenerationContextBuilder::build_from_entity_source(
                request.schema,
                request.route_base,
                &entity_source,
            )?;
            let renderer = TemplateRenderer::new(&FRONTEND_API_TEMPLATES)?;
            let api_source = renderer.render(FRONTEND_API_TEMPLATE_NAME, &context)?;
            let api_type_source = renderer.render(FRONTEND_API_TYPE_TEMPLATE_NAME, &context)?;
            (context, api_source, api_type_source)
        };

        tokio::fs::write(&paths.api_file, api_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write frontend api file `{}`", paths.api_file.display()),
                    error,
                )
            })?;
        tokio::fs::write(&paths.api_type_file, api_type_source)
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "write frontend api type file `{}`",
                        paths.api_type_file.display()
                    ),
                    error,
                )
            })?;

        Ok(GenerateFrontendApiResult {
            table: context.table.name.clone(),
            route_base: context.table.route_base.clone(),
            namespace: context.names.ts_namespace.clone(),
            api_file: paths.api_file,
            api_type_file: paths.api_type_file,
        })
    }

    fn build_paths(
        &self,
        table: &str,
        route_base_override: Option<&str>,
        output_dir: Option<&str>,
        target_preset: FrontendTargetPreset,
    ) -> Result<GeneratedPaths, McpError> {
        let route_base = route_base_override
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default_route_base(table));
        let file_stem = route_base.replace('_', "-");
        let layout = target_preset.resolve_bundle_layout(&self.workspace_root, output_dir)?;
        Ok(GeneratedPaths {
            entity_file: self
                .workspace_root
                .join(MODEL_ENTITY_DIR)
                .join(format!("{table}.rs")),
            api_file: layout.api_dir.join(format!("{file_stem}.ts")),
            api_type_file: layout.api_type_dir.join(format!("{file_stem}.d.ts")),
        })
    }

    async fn ensure_parent_dirs(&self, paths: &GeneratedPaths) -> Result<(), McpError> {
        for directory in [paths.api_file.parent(), paths.api_type_file.parent()]
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

#[cfg(test)]
mod tests {
    use crate::tools::{
        frontend_target::DEFAULT_SUMMER_FRONTEND_ROOT_DIR,
        test_support::{SAMPLE_ROLE_ENTITY_SOURCE, sample_role_schema},
    };

    use super::*;

    #[tokio::test]
    async fn generator_writes_frontend_api_files() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-api-generator-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        for dir in [
            root.join(DEFAULT_SUMMER_FRONTEND_ROOT_DIR).join("api"),
            root.join(DEFAULT_SUMMER_FRONTEND_ROOT_DIR).join("api_type"),
            root.join(MODEL_ENTITY_DIR),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }

        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_role.rs"),
            SAMPLE_ROLE_ENTITY_SOURCE,
        )
        .unwrap();

        let generator = FrontendApiGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendApiRequest {
                schema: sample_role_schema(),
                overwrite: true,
                route_base: None,
                output_dir: None,
                target_preset: FrontendTargetPreset::SummerMcp,
            })
            .await
            .unwrap();

        let api_file = std::fs::read_to_string(&result.api_file).unwrap();
        assert!(api_file.contains("fetchGetRoleList"));
        assert!(api_file.contains("url: '/api/role/list'"));

        let api_type_file = std::fs::read_to_string(&result.api_type_file).unwrap();
        assert!(api_type_file.contains("namespace Role"));
        assert!(api_type_file.contains("interface RoleVo"));
        assert!(api_type_file.contains("roleName: string"));
        assert!(api_type_file.contains("createTime: string"));
        assert!(api_type_file.contains("createTimeStart?: string"));
        assert!(api_type_file.contains("createTimeEnd?: string"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_supports_art_design_pro_layout() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-api-generator-adp-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_role.rs"),
            SAMPLE_ROLE_ENTITY_SOURCE,
        )
        .unwrap();

        let generator = FrontendApiGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendApiRequest {
                schema: sample_role_schema(),
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/art-design-pro".to_string()),
                target_preset: FrontendTargetPreset::ArtDesignPro,
            })
            .await
            .unwrap();

        assert_eq!(
            result.api_file,
            root.join("generated/art-design-pro/src/api/role.ts")
        );
        assert_eq!(
            result.api_type_file,
            root.join("generated/art-design-pro/src/types/api/role.d.ts")
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
