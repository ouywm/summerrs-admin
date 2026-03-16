use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rmcp::ErrorData as McpError;
use tokio::process::Command;

use crate::tools::{
    support::{io_error, sync_mod_file, workspace_root},
    validation::{
        GenerationValidationSummary, validate_mod_registration, validate_rust_sources,
        validate_workspace_cargo_check_for_generated_output,
    },
};

const DEFAULT_DATABASE_SCHEMA: &str = "public";
const DEFAULT_ENTITY_DIR: &str = "crates/model/src/entity";
const DEFAULT_CLI_BIN: &str = "sea-orm-cli";
const CLI_BIN_ENV: &str = "SUMMER_MCP_SEA_ORM_CLI_BIN";
const DATABASE_URL_ENV: &str = "DATABASE_URL";

#[derive(Debug, Clone)]
pub struct EntityGenerator {
    workspace_root: PathBuf,
    default_database_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GenerateEntityRequest {
    pub table: String,
    pub overwrite: bool,
    pub output_dir: Option<String>,
    pub database_url: Option<String>,
    pub database_schema: Option<String>,
    pub cli_bin: Option<String>,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct GenerateEntityResult {
    pub table: String,
    pub entity_file: PathBuf,
    pub mod_file: PathBuf,
    pub overwritten: bool,
    pub database_schema: String,
    pub cli_bin: String,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone)]
struct SeaOrmCliGenerateEntityCommand<'a> {
    cli_bin: &'a str,
    database_url: &'a str,
    database_schema: &'a str,
    table: &'a str,
    output_dir: &'a Path,
    options: SeaOrmCliGenerateEntityOptions,
}

#[derive(Debug, Clone)]
struct SeaOrmCliGenerateEntityOptions {
    entity_format: &'static str,
    serde_mode: &'static str,
    prelude_mode: &'static str,
    date_time_crate: &'static str,
    banner_version: &'static str,
    impl_active_model_behavior: bool,
}

impl Default for SeaOrmCliGenerateEntityOptions {
    fn default() -> Self {
        Self {
            entity_format: "dense",
            serde_mode: "both",
            prelude_mode: "none",
            date_time_crate: "chrono",
            banner_version: "off",
            impl_active_model_behavior: true,
        }
    }
}

impl<'a> SeaOrmCliGenerateEntityCommand<'a> {
    fn new(
        cli_bin: &'a str,
        database_url: &'a str,
        database_schema: &'a str,
        table: &'a str,
        output_dir: &'a Path,
    ) -> Self {
        Self {
            cli_bin,
            database_url,
            database_schema,
            table,
            output_dir,
            options: SeaOrmCliGenerateEntityOptions::default(),
        }
    }

    fn args(&self) -> Vec<OsString> {
        let mut args = vec![
            OsString::from("generate"),
            OsString::from("entity"),
            OsString::from("--database-url"),
            OsString::from(self.database_url),
            OsString::from("--database-schema"),
            OsString::from(self.database_schema),
            OsString::from("--tables"),
            OsString::from(self.table),
            OsString::from("--output-dir"),
            self.output_dir.as_os_str().to_owned(),
            OsString::from("--entity-format"),
            OsString::from(self.options.entity_format),
            OsString::from("--with-serde"),
            OsString::from(self.options.serde_mode),
            OsString::from("--with-prelude"),
            OsString::from(self.options.prelude_mode),
            OsString::from("--date-time-crate"),
            OsString::from(self.options.date_time_crate),
        ];

        if self.options.impl_active_model_behavior {
            args.push(OsString::from("--impl-active-model-behavior"));
        }

        args.push(OsString::from("--banner-version"));
        args.push(OsString::from(self.options.banner_version));
        args
    }

    async fn output(&self) -> Result<std::process::Output, std::io::Error> {
        let mut command = Command::new(self.cli_bin);
        command.args(self.args());
        command.output().await
    }
}

impl EntityGenerator {
    pub fn new(default_database_url: Option<String>) -> Result<Self, McpError> {
        Ok(Self {
            workspace_root: workspace_root()?,
            default_database_url,
        })
    }

    #[cfg(test)]
    fn with_workspace_root(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            default_database_url: None,
        }
    }

    pub async fn generate(
        &self,
        request: GenerateEntityRequest,
    ) -> Result<GenerateEntityResult, McpError> {
        let entity_dir = self.resolve_entity_dir(request.output_dir.as_deref())?;
        tokio::fs::create_dir_all(&entity_dir)
            .await
            .map_err(|error| {
                io_error(
                    format!("create entity output directory `{}`", entity_dir.display()),
                    error,
                )
            })?;

        let entity_file = entity_dir.join(format!("{}.rs", request.table));
        let mod_file = entity_dir.join("mod.rs");
        let existed = tokio::fs::try_exists(&entity_file).await.map_err(|error| {
            io_error(
                format!("check entity file `{}`", entity_file.display()),
                error,
            )
        })?;
        if existed && !request.overwrite {
            return Err(McpError::invalid_params(
                format!(
                    "entity file `{}` already exists; set overwrite=true to regenerate",
                    entity_file.display()
                ),
                None,
            ));
        }

        let temp_output_dir = self.temp_output_dir(&request.table)?;
        tokio::fs::create_dir_all(&temp_output_dir)
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "create temporary output directory `{}`",
                        temp_output_dir.display()
                    ),
                    error,
                )
            })?;

        let cleanup_guard = TempDirGuard::new(temp_output_dir.clone());
        let cli_bin = request
            .cli_bin
            .clone()
            .or_else(|| std::env::var(CLI_BIN_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CLI_BIN.to_string());
        let database_url = self.resolve_database_url(request.database_url.as_deref())?;
        let database_schema = request
            .database_schema
            .clone()
            .unwrap_or_else(|| DEFAULT_DATABASE_SCHEMA.to_string());

        run_sea_orm_cli(
            &cli_bin,
            &database_url,
            &database_schema,
            &request.table,
            &temp_output_dir,
        )
        .await?;

        let generated_entity_file = temp_output_dir.join(format!("{}.rs", request.table));
        let generated_contents = tokio::fs::read_to_string(&generated_entity_file)
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "read generated entity file `{}`",
                        generated_entity_file.display()
                    ),
                    error,
                )
            })?;
        let normalized_contents =
            normalize_generated_entity_contents(&generated_contents, &request.table);
        tokio::fs::write(&entity_file, normalized_contents)
            .await
            .map_err(|error| {
                io_error(
                    format!("write entity file `{}`", entity_file.display()),
                    error,
                )
            })?;

        sync_mod_file(&mod_file, &request.table).await?;
        let validation = self
            .validate_generated_output(&request, &entity_file, &mod_file)
            .await;
        drop(cleanup_guard);

        Ok(GenerateEntityResult {
            table: request.table,
            entity_file,
            mod_file,
            overwritten: existed,
            database_schema,
            cli_bin,
            validation,
        })
    }

    fn resolve_entity_dir(&self, output_dir: Option<&str>) -> Result<PathBuf, McpError> {
        let path = match output_dir {
            Some(output_dir) => {
                let path = PathBuf::from(output_dir);
                if path.is_absolute() {
                    path
                } else {
                    self.workspace_root.join(path)
                }
            }
            None => self.workspace_root.join(DEFAULT_ENTITY_DIR),
        };

        Ok(path)
    }

    fn resolve_database_url(
        &self,
        explicit_database_url: Option<&str>,
    ) -> Result<String, McpError> {
        explicit_database_url
            .map(ToOwned::to_owned)
            .or_else(|| self.default_database_url.clone())
            .or_else(|| std::env::var(DATABASE_URL_ENV).ok())
            .ok_or_else(|| {
                McpError::invalid_params(
                    "database_url is required for generate_entity_from_table; provide it explicitly or start the standalone MCP with --database-url",
                    None,
                )
            })
    }

    fn temp_output_dir(&self, table: &str) -> Result<PathBuf, McpError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .as_millis();
        Ok(std::env::temp_dir().join(format!(
            "summer-mcp-entity-gen-{table}-{timestamp}-{}",
            std::process::id()
        )))
    }

    async fn validate_generated_output(
        &self,
        request: &GenerateEntityRequest,
        entity_file: &Path,
        mod_file: &Path,
    ) -> GenerationValidationSummary {
        let checks = vec![
            validate_rust_sources("rust_syntax", &[entity_file.to_path_buf()]).await,
            validate_mod_registration("entity_module_registration", mod_file, &request.table).await,
            validate_workspace_cargo_check_for_generated_output(
                &self.workspace_root,
                request.output_dir.as_deref(),
                &["model"],
            )
            .await,
        ];

        GenerationValidationSummary::from_checks(checks)
    }
}

async fn run_sea_orm_cli(
    cli_bin: &str,
    database_url: &str,
    database_schema: &str,
    table: &str,
    output_dir: &Path,
) -> Result<(), McpError> {
    let command = SeaOrmCliGenerateEntityCommand::new(
        cli_bin,
        database_url,
        database_schema,
        table,
        output_dir,
    );
    let output = command.output().await.map_err(|error| {
        McpError::internal_error(
            format!("failed to run `{cli_bin}` for table `{table}`: {error}"),
            None,
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(McpError::internal_error(
        format!(
            "sea-orm-cli failed for table `{table}` (status {}): stderr=`{stderr}` stdout=`{stdout}`",
            output.status
        ),
        None,
    ))
}
fn normalize_generated_entity_contents(contents: &str, table: &str) -> String {
    contents
        .replace(
            &format!("#[sea_orm(schema_name = \"public\", table_name = \"{table}\")]"),
            &format!("#[sea_orm(table_name = \"{table}\")]"),
        )
        .trim()
        .to_string()
        + "\n"
}

struct TempDirGuard {
    path: Option<PathBuf>,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            // Move the synchronous remove_dir_all off the tokio executor thread.
            std::thread::spawn(move || {
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(
                            "failed to remove temporary entity generation directory `{}`: {error}",
                            path.display()
                        );
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tools::validation::ValidationStatus;

    use super::*;

    #[test]
    fn normalize_generated_entity_contents_removes_public_schema_annotation() {
        let input = r#"
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[sea_orm(schema_name = "public", table_name = "sys_role")]
pub struct Model;
"#;

        let normalized = normalize_generated_entity_contents(input, "sys_role");
        assert!(normalized.contains("#[sea_orm(table_name = \"sys_role\")]"));
        assert!(!normalized.contains("schema_name"));
    }

    #[test]
    fn sea_orm_cli_generate_entity_command_uses_expected_args() {
        let output_dir = Path::new("/tmp/summer-mcp-entity-output");
        let command = SeaOrmCliGenerateEntityCommand::new(
            "sea-orm-cli",
            "postgres://demo",
            "public",
            "sys_role",
            output_dir,
        );

        let args = command
            .args()
            .into_iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "generate",
                "entity",
                "--database-url",
                "postgres://demo",
                "--database-schema",
                "public",
                "--tables",
                "sys_role",
                "--output-dir",
                "/tmp/summer-mcp-entity-output",
                "--entity-format",
                "dense",
                "--with-serde",
                "both",
                "--with-prelude",
                "none",
                "--date-time-crate",
                "chrono",
                "--impl-active-model-behavior",
                "--banner-version",
                "off",
            ]
        );
    }

    #[tokio::test]
    async fn entity_validation_skips_workspace_check_for_explicit_output_dir() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-entity-validation-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        let entity_dir = root.join("generated/entity");
        std::fs::create_dir_all(&entity_dir).unwrap();
        let entity_file = entity_dir.join("sys_role.rs");
        let mod_file = entity_dir.join("mod.rs");
        std::fs::write(
            &entity_file,
            "use sea_orm::entity::prelude::*;\n\n#[sea_orm::model]\npub struct Model {\n    pub id: i64,\n}\n",
        )
        .unwrap();
        std::fs::write(&mod_file, "pub mod sys_role;\n").unwrap();

        let generator = EntityGenerator::with_workspace_root(root.clone());
        let validation = generator
            .validate_generated_output(
                &GenerateEntityRequest {
                    table: "sys_role".to_string(),
                    overwrite: true,
                    output_dir: Some(entity_dir.display().to_string()),
                    database_url: None,
                    database_schema: None,
                    cli_bin: None,
                },
                &entity_file,
                &mod_file,
            )
            .await;

        assert_eq!(validation.status, ValidationStatus::Passed);
        assert!(
            validation
                .checks
                .iter()
                .any(|check| check.name == "rust_workspace_check"
                    && check.status == ValidationStatus::Skipped)
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
