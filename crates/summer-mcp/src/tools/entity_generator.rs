use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rmcp::ErrorData as McpError;
use syn::{Fields, Item, Type, parse_file};
use tokio::process::Command;

use crate::{
    table_tools::schema::TableSchema,
    tools::{
        entity_enum_upgrader::{EntityEnumUpgradeRequest, EntityEnumUpgrader},
        support::{
            io_error, is_create_timestamp_field_name, is_update_timestamp_field_name,
            sync_mod_file, workspace_root,
        },
        validation::{
            GenerationValidationSummary, validate_mod_registration, validate_rust_sources,
            validate_workspace_cargo_check_for_generated_output,
        },
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
    pub schema: Option<TableSchema>,
    pub enum_name_overrides: BTreeMap<String, String>,
    pub variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
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
    pub enum_upgrade_changed: bool,
    pub enum_upgrade_fields: Vec<String>,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Default)]
struct AutoEntityEnumUpgradeSummary {
    changed: bool,
    fields: Vec<String>,
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

#[derive(Debug, Clone)]
struct AuditTimestampFieldContext {
    name: String,
    optional: bool,
}

#[derive(Debug, Clone, Default)]
struct AuditTimestampBehaviorContext {
    create_field: Option<AuditTimestampFieldContext>,
    update_field: Option<AuditTimestampFieldContext>,
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
        let normalized_contents = request
            .schema
            .as_ref()
            .map(|schema| inject_entity_field_comments(&normalized_contents, schema))
            .unwrap_or(normalized_contents);
        tokio::fs::write(&entity_file, normalized_contents)
            .await
            .map_err(|error| {
                io_error(
                    format!("write entity file `{}`", entity_file.display()),
                    error,
                )
            })?;

        sync_mod_file(&mod_file, &request.table).await?;
        let enum_upgrade = self.auto_upgrade_entity_enums(&request).await?;
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
            enum_upgrade_changed: enum_upgrade.changed,
            enum_upgrade_fields: enum_upgrade.fields,
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

    async fn auto_upgrade_entity_enums(
        &self,
        request: &GenerateEntityRequest,
    ) -> Result<AutoEntityEnumUpgradeSummary, McpError> {
        let Some(schema) = request.schema.clone() else {
            return Ok(AutoEntityEnumUpgradeSummary::default());
        };

        let upgrader = EntityEnumUpgrader::with_workspace_root(self.workspace_root.clone());
        let result = upgrader
            .apply(EntityEnumUpgradeRequest {
                schema,
                route_base: None,
                output_dir: request.output_dir.clone(),
                fields: None,
                enum_name_overrides: request.enum_name_overrides.clone(),
                variant_name_overrides: request.variant_name_overrides.clone(),
            })
            .await?;

        Ok(AutoEntityEnumUpgradeSummary {
            changed: result.preview.changed,
            fields: result
                .preview
                .plan
                .fields
                .into_iter()
                .map(|field| field.field_name)
                .collect(),
        })
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
    let normalized = contents
        .replace(
            &format!("#[sea_orm(schema_name = \"public\", table_name = \"{table}\")]"),
            &format!("#[sea_orm(table_name = \"{table}\")]"),
        )
        .trim()
        .to_string();
    inject_audit_timestamp_behavior(&normalized) + "\n"
}

fn inject_entity_field_comments(contents: &str, schema: &TableSchema) -> String {
    let field_comments = schema
        .columns
        .iter()
        .filter_map(|column| {
            normalize_entity_doc_comment(column.comment.as_deref())
                .map(|comment| (column.name.as_str(), comment))
        })
        .collect::<BTreeMap<_, _>>();

    if field_comments.is_empty() {
        return contents.to_string();
    }

    let mut output = Vec::new();
    let mut pending_attrs = Vec::new();
    let mut inside_model = false;

    for line in contents.lines() {
        let trimmed = line.trim_start();

        if !inside_model {
            if trimmed.starts_with("pub struct Model") {
                inside_model = true;
            }
            output.push(line.to_string());
            continue;
        }

        if trimmed == "}" {
            output.append(&mut pending_attrs);
            output.push(line.to_string());
            inside_model = false;
            continue;
        }

        if trimmed.starts_with("#[") {
            pending_attrs.push(line.to_string());
            continue;
        }

        if let Some(field_name) = parse_entity_model_field_name(trimmed) {
            if let Some(comment) = field_comments.get(field_name) {
                output.push(format!("    /// {comment}"));
            }
            output.append(&mut pending_attrs);
            output.push(line.to_string());
            continue;
        }

        output.append(&mut pending_attrs);
        output.push(line.to_string());
    }

    if !pending_attrs.is_empty() {
        output.append(&mut pending_attrs);
    }

    let mut rendered = output.join("\n");
    if contents.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

fn normalize_entity_doc_comment(comment: Option<&str>) -> Option<String> {
    let normalized = comment?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn parse_entity_model_field_name(line: &str) -> Option<&str> {
    let field = line.strip_prefix("pub ")?;
    let (name, _) = field.split_once(':')?;
    Some(name.trim())
}

fn inject_audit_timestamp_behavior(contents: &str) -> String {
    let Some(context) = parse_audit_timestamp_behavior_context(contents) else {
        return contents.to_string();
    };

    let behavior_block = render_audit_timestamp_behavior(&context);
    let mut normalized = contents.replace(
        "impl ActiveModelBehavior for ActiveModel {}",
        &behavior_block,
    );

    if !normalized.contains("use sea_orm::Set;") {
        normalized = inject_set_import(&normalized);
    }

    normalized
}

fn parse_audit_timestamp_behavior_context(contents: &str) -> Option<AuditTimestampBehaviorContext> {
    let file = parse_file(contents).ok()?;
    let model = file.items.iter().find_map(|item| match item {
        Item::Struct(item) if item.ident == "Model" => Some(item),
        _ => None,
    })?;
    let Fields::Named(fields) = &model.fields else {
        return None;
    };

    let mut context = AuditTimestampBehaviorContext::default();
    for field in &fields.named {
        let Some(ident) = &field.ident else {
            continue;
        };
        let field_name = ident.to_string();
        let optional = option_inner_type(&field.ty).is_some();
        let raw_type = option_inner_type(&field.ty).unwrap_or(&field.ty);
        if !is_entity_datetime_type(raw_type) {
            continue;
        }

        if context.update_field.is_none() && is_update_timestamp_field_name(&field_name) {
            context.update_field = Some(AuditTimestampFieldContext {
                name: field_name,
                optional,
            });
            continue;
        }

        if context.create_field.is_none() && is_create_timestamp_field_name(&field_name) {
            context.create_field = Some(AuditTimestampFieldContext {
                name: field_name,
                optional,
            });
        }
    }

    if context.create_field.is_none() && context.update_field.is_none() {
        None
    } else {
        Some(context)
    }
}

fn render_audit_timestamp_behavior(context: &AuditTimestampBehaviorContext) -> String {
    let mut lines = vec![
        "#[async_trait::async_trait]".to_string(),
        "impl ActiveModelBehavior for ActiveModel {".to_string(),
        "    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>"
            .to_string(),
        "    where".to_string(),
        "        C: ConnectionTrait,".to_string(),
        "    {".to_string(),
        "        let now = chrono::Local::now().naive_local();".to_string(),
    ];

    if let Some(field) = &context.update_field {
        lines.push(format!(
            "        self.{} = Set({});",
            field.name,
            render_now_assignment(field.optional)
        ));
    }

    if let Some(field) = &context.create_field {
        lines.push("        if insert {".to_string());
        lines.push(format!(
            "            self.{} = Set({});",
            field.name,
            render_now_assignment(field.optional)
        ));
        lines.push("        }".to_string());
    }

    lines.push("        Ok(self)".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}

fn render_now_assignment(optional: bool) -> &'static str {
    if optional { "Some(now)" } else { "now" }
}

fn inject_set_import(contents: &str) -> String {
    const PRELUDE_USE: &str = "use sea_orm::entity::prelude::*;";
    if contents.contains(PRELUDE_USE) {
        contents.replacen(PRELUDE_USE, &format!("{PRELUDE_USE}\nuse sea_orm::Set;"), 1)
    } else {
        format!("use sea_orm::Set;\n{contents}")
    }
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    if type_path.qself.is_some() || type_path.path.segments.len() != 1 {
        return None;
    }
    let segment = &type_path.path.segments[0];
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let first = arguments.args.first()?;
    let syn::GenericArgument::Type(inner) = first else {
        return None;
    };
    Some(inner)
}

fn is_entity_datetime_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "DateTime")
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
                if let Err(error) = std::fs::remove_dir_all(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(
                        "failed to remove temporary entity generation directory `{}`: {error}",
                        path.display()
                    );
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
    fn normalize_generated_entity_contents_injects_audit_timestamp_behavior() {
        let input = r#"
use sea_orm::entity::prelude::*;

#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub create_time: DateTime,
    pub update_time: DateTime,
}

impl ActiveModelBehavior for ActiveModel {}
"#;

        let normalized = normalize_generated_entity_contents(input, "sys_role");
        assert!(normalized.contains("use sea_orm::Set;"));
        assert!(normalized.contains("#[async_trait::async_trait]"));
        assert!(normalized.contains("self.update_time = Set(now);"));
        assert!(normalized.contains("self.create_time = Set(now);"));
        assert!(!normalized.contains("impl ActiveModelBehavior for ActiveModel {}"));
    }

    #[test]
    fn normalize_generated_entity_contents_handles_nullable_audit_timestamps() {
        let input = r#"
use sea_orm::entity::prelude::*;

#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub created_at: Option<DateTime>,
    pub updated_at: Option<DateTime>,
}

impl ActiveModelBehavior for ActiveModel {}
"#;

        let normalized = normalize_generated_entity_contents(input, "biz_article");
        assert!(normalized.contains("self.updated_at = Set(Some(now));"));
        assert!(normalized.contains("self.created_at = Set(Some(now));"));
    }

    #[test]
    fn inject_entity_field_comments_places_docs_above_field_attributes() {
        let contents = r#"use sea_orm::entity::prelude::*;

#[sea_orm::model]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub config_name: String,
}
"#;
        let schema = TableSchema {
            schema: "public".to_string(),
            table: "sys_config".to_string(),
            comment: Some("系统参数配置表".to_string()),
            primary_key: vec!["id".to_string()],
            columns: vec![
                crate::table_tools::schema::TableColumnSchema {
                    name: "id".to_string(),
                    pg_type: "bigint".to_string(),
                    nullable: false,
                    primary_key: true,
                    hidden_on_read: false,
                    writable_on_create: false,
                    writable_on_update: false,
                    default_value: Some("nextval(...)".to_string()),
                    comment: Some("配置ID".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                crate::table_tools::schema::TableColumnSchema {
                    name: "config_name".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("配置名称".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        };

        let rendered = inject_entity_field_comments(contents, &schema);
        assert!(rendered.contains("    /// 配置ID\n    #[sea_orm(primary_key)]\n    pub id: i64,"));
        assert!(rendered.contains("    /// 配置名称\n    pub config_name: String,"));
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
                    schema: None,
                    enum_name_overrides: BTreeMap::new(),
                    variant_name_overrides: BTreeMap::new(),
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

    #[tokio::test]
    async fn auto_upgrade_entity_enums_promotes_comment_backed_numeric_fields() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-entity-enum-auto-upgrade-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        let entity_dir = root.join("generated/entity");
        std::fs::create_dir_all(&entity_dir).unwrap();
        let entity_file = entity_dir.join("sys_config.rs");
        std::fs::write(
            &entity_file,
            r#"use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub value_type: i16,
}

impl ActiveModelBehavior for ActiveModel {}
"#,
        )
        .unwrap();

        let generator = EntityGenerator::with_workspace_root(root.clone());
        let summary = generator
            .auto_upgrade_entity_enums(&GenerateEntityRequest {
                table: "sys_config".to_string(),
                overwrite: true,
                output_dir: Some(entity_dir.display().to_string()),
                database_url: None,
                database_schema: None,
                cli_bin: None,
                schema: Some(TableSchema {
                    schema: "public".to_string(),
                    table: "sys_config".to_string(),
                    comment: Some("系统参数配置表".to_string()),
                    primary_key: vec!["id".to_string()],
                    columns: vec![
                        crate::table_tools::schema::TableColumnSchema {
                            name: "id".to_string(),
                            pg_type: "bigint".to_string(),
                            nullable: false,
                            primary_key: true,
                            hidden_on_read: false,
                            writable_on_create: false,
                            writable_on_update: false,
                            default_value: Some("nextval(...)".to_string()),
                            comment: Some("配置ID".to_string()),
                            is_identity: false,
                            is_generated: false,
                            enum_values: None,
                        },
                        crate::table_tools::schema::TableColumnSchema {
                            name: "value_type".to_string(),
                            pg_type: "smallint".to_string(),
                            nullable: false,
                            primary_key: false,
                            hidden_on_read: false,
                            writable_on_create: true,
                            writable_on_update: true,
                            default_value: Some("1".to_string()),
                            comment: Some(
                                "值类型：1=文本 2=数字 3=布尔 4=文本域 5=下拉单选 6=JSON 7=密码 8=图片"
                                    .to_string(),
                            ),
                            is_identity: false,
                            is_generated: false,
                            enum_values: None,
                        },
                    ],
                    indexes: vec![],
                    foreign_keys: vec![],
                    check_constraints: vec![],
                }),
                enum_name_overrides: BTreeMap::new(),
                variant_name_overrides: BTreeMap::new(),
            })
            .await
            .unwrap();

        let upgraded = std::fs::read_to_string(&entity_file).unwrap();
        assert!(summary.changed);
        assert_eq!(summary.fields, vec!["value_type"]);
        assert!(upgraded.contains("pub enum ValueType"));
        assert!(upgraded.contains("#[sea_orm(rs_type = \"i16\", db_type = \"SmallInteger\")]"));
        assert!(upgraded.contains("Text = 1"));
        assert!(upgraded.contains("Number = 2"));
        assert!(upgraded.contains("Boolean = 3"));
        assert!(upgraded.contains("Textarea = 4"));
        assert!(upgraded.contains("Select = 5"));
        assert!(upgraded.contains("#[sea_orm(num_value = 6)]"));
        assert!(upgraded.contains("Password = 7"));
        assert!(upgraded.contains("Image = 8"));
        assert!(upgraded.contains("pub value_type: ValueType,"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
