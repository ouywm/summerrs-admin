use std::{
    collections::{BTreeMap, BTreeSet},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use quote::ToTokens;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use syn::{Fields, File, Item, ItemStruct, Type, parse_file};
use tokio::process::Command;

use crate::{
    error_model::{internal_error, invalid_params_error},
    output_contract::{ArtifactBundleSummary, build_artifact_bundle, generator_artifact_mode},
    table_tools::schema::{TableColumnSchema, TableSchema, ensure_valid_identifier},
    tools::{
        enum_semantics::{EnumDraftSpec, EnumSemanticSource},
        generation_context::{CrudGenerationContextBuilder, EnumOptionValueKind},
        support::{io_error, resolve_output_dir, workspace_root},
        validation::{
            GenerationValidationSummary, validate_rust_sources,
            validate_workspace_cargo_check_for_generated_output,
        },
    },
};

const DEFAULT_ENTITY_DIR: &str = "crates/model/src/entity";
static TEMP_RUSTFMT_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct EntityEnumUpgrader {
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EntityEnumUpgradeRequest {
    pub schema: TableSchema,
    pub route_base: Option<String>,
    pub output_dir: Option<String>,
    pub fields: Option<Vec<String>>,
    pub enum_name_overrides: BTreeMap<String, String>,
    pub variant_name_overrides: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct EntityEnumUpgradePreview {
    pub table: String,
    pub route_base: String,
    pub entity_file: PathBuf,
    pub changed: bool,
    pub plan: EntityEnumUpgradePlan,
    pub rendered_source: String,
    pub artifacts: ArtifactBundleSummary,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct ApplyEntityEnumUpgradeResult {
    pub preview: EntityEnumUpgradePreview,
    pub validation: GenerationValidationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityEnumNameSource {
    Draft,
    Override,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityEnumVariantNameSource {
    Override,
    LabelAscii,
    CommonLabel,
    ValueFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityEnumDefinitionAction {
    Create,
    ReuseExisting,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EntityEnumVariantPlan {
    pub label: String,
    pub value: String,
    pub value_kind: EnumOptionValueKind,
    pub rust_variant_name: String,
    pub name_source: EntityEnumVariantNameSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EntityEnumFieldPlan {
    pub field_name: String,
    pub field_label: String,
    pub source: EnumSemanticSource,
    pub definition_action: EntityEnumDefinitionAction,
    pub from_rust_type: String,
    pub to_rust_type: String,
    pub rust_enum_name: String,
    pub enum_name_source: EntityEnumNameSource,
    pub sea_orm_rs_type: String,
    pub sea_orm_db_type: String,
    pub sea_orm_enum_name: Option<String>,
    pub variants: Vec<EntityEnumVariantPlan>,
    pub needs_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EntityEnumUpgradePlan {
    pub fields: Vec<EntityEnumFieldPlan>,
    pub warnings: Vec<String>,
}

struct PreparedUpgrade {
    preview: EntityEnumUpgradePreview,
}

struct PreparedUpgradeRender {
    route_base: String,
    changed: bool,
    plan: EntityEnumUpgradePlan,
    rendered_source: String,
}

impl EntityEnumUpgrader {
    pub fn new() -> Result<Self, McpError> {
        Ok(Self {
            workspace_root: workspace_root()?,
        })
    }

    #[cfg(test)]
    fn with_workspace_root(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub async fn plan(
        &self,
        request: EntityEnumUpgradeRequest,
    ) -> Result<EntityEnumUpgradePreview, McpError> {
        Ok(self.prepare(request).await?.preview)
    }

    pub async fn apply(
        &self,
        request: EntityEnumUpgradeRequest,
    ) -> Result<ApplyEntityEnumUpgradeResult, McpError> {
        let prepared = self.prepare(request.clone()).await?;
        if prepared.preview.changed {
            tokio::fs::write(
                &prepared.preview.entity_file,
                &prepared.preview.rendered_source,
            )
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "write upgraded entity file `{}`",
                        prepared.preview.entity_file.display()
                    ),
                    error,
                )
            })?;
            try_rustfmt_file(&prepared.preview.entity_file).await?;
        }

        let validation = self
            .validate_output(&request, &prepared.preview.entity_file)
            .await;

        Ok(ApplyEntityEnumUpgradeResult {
            preview: prepared.preview,
            validation,
        })
    }

    async fn prepare(
        &self,
        request: EntityEnumUpgradeRequest,
    ) -> Result<PreparedUpgrade, McpError> {
        self.validate_request(&request)?;

        let entity_dir = resolve_output_dir(
            &self.workspace_root,
            request.output_dir.as_deref(),
            DEFAULT_ENTITY_DIR,
        );
        let entity_file = entity_dir.join(format!("{}.rs", request.schema.table));
        let entity_source = tokio::fs::read_to_string(&entity_file)
            .await
            .map_err(|error| {
                io_error(
                    format!("read entity file `{}`", entity_file.display()),
                    error,
                )
            })?;
        let prepared_render = prepare_upgrade_render(&request, &entity_file, &entity_source)?;
        let rendered_source = if prepared_render.changed {
            format_rust_source(&prepared_render.rendered_source).await?
        } else {
            prepared_render.rendered_source
        };

        let output_root = entity_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let artifacts = build_artifact_bundle(
            generator_artifact_mode(request.output_dir.as_deref()),
            &output_root,
            [("entity_file", entity_file.as_path())],
        );

        Ok(PreparedUpgrade {
            preview: EntityEnumUpgradePreview {
                table: request.schema.table,
                route_base: prepared_render.route_base,
                entity_file,
                changed: prepared_render.changed,
                plan: prepared_render.plan,
                rendered_source,
                artifacts,
            },
        })
    }

    fn validate_request(&self, request: &EntityEnumUpgradeRequest) -> Result<(), McpError> {
        if let Some(fields) = &request.fields {
            for field in fields {
                ensure_valid_identifier(field, "fields")?;
            }
        }
        for (field, enum_name) in &request.enum_name_overrides {
            ensure_valid_identifier(field, "enum_name_overrides field")?;
            ensure_valid_rust_ident(enum_name, "enum_name_overrides value")?;
        }
        for (field, variants) in &request.variant_name_overrides {
            ensure_valid_identifier(field, "variant_name_overrides field")?;
            for variant_name in variants.values() {
                ensure_valid_rust_ident(variant_name, "variant_name_overrides value")?;
            }
        }
        Ok(())
    }

    async fn validate_output(
        &self,
        request: &EntityEnumUpgradeRequest,
        entity_file: &Path,
    ) -> GenerationValidationSummary {
        let checks = vec![
            validate_rust_sources("rust_syntax", &[entity_file.to_path_buf()]).await,
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

fn prepare_upgrade_render(
    request: &EntityEnumUpgradeRequest,
    entity_file: &Path,
    entity_source: &str,
) -> Result<PreparedUpgradeRender, McpError> {
    let context = CrudGenerationContextBuilder::build_from_entity_source(
        request.schema.clone(),
        request.route_base.clone(),
        entity_source,
    )?;
    let route_base = context.table.route_base.clone();
    let mut parsed_file = parse_file(entity_source).map_err(|error| {
        internal_error(
            "parse_entity_file_failed",
            "Failed to parse entity file",
            Some("Check that the current entity file contains valid Rust syntax."),
            Some(format!(
                "failed to parse `{}`: {error}",
                entity_file.display()
            )),
            Some(serde_json::json!({ "entity_file": entity_file.display().to_string() })),
        )
    })?;

    let planned_fields = build_field_plans(request, &request.schema, &context, &parsed_file)?;
    let warnings = build_plan_warnings(request, &planned_fields);
    let changed = !planned_fields.is_empty();

    if changed {
        apply_ast_upgrade(&mut parsed_file, &planned_fields)?;
    }

    let rendered_source = if changed {
        parsed_file.to_token_stream().to_string()
    } else {
        entity_source.to_string()
    };

    Ok(PreparedUpgradeRender {
        route_base,
        changed,
        plan: EntityEnumUpgradePlan {
            fields: planned_fields,
            warnings,
        },
        rendered_source,
    })
}

fn build_field_plans(
    request: &EntityEnumUpgradeRequest,
    schema: &TableSchema,
    context: &super::generation_context::CrudGenerationContext,
    parsed_file: &File,
) -> Result<Vec<EntityEnumFieldPlan>, McpError> {
    let selected = request
        .fields
        .as_ref()
        .map(|fields| fields.iter().cloned().collect::<BTreeSet<_>>());
    let field_map = context
        .fields
        .iter()
        .map(|field| (field.name.clone(), field))
        .collect::<BTreeMap<_, _>>();
    let existing_enums = parsed_file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(item) => Some(item.ident.to_string()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut plans = Vec::new();
    for draft in &context.enum_drafts {
        let Some(field) = field_map.get(&draft.field_name).copied() else {
            continue;
        };
        if field.type_info.enum_source == Some(EnumSemanticSource::EntityEnum) {
            continue;
        }
        if selected
            .as_ref()
            .is_some_and(|selected| !selected.contains(&draft.field_name))
        {
            continue;
        }

        let column = schema.column(&draft.field_name).ok_or_else(|| {
            internal_error(
                "schema_column_not_found",
                "Schema column not found",
                Some("Re-read the live table schema before retrying."),
                Some(format!(
                    "column `{}` was not found in schema `{}`",
                    draft.field_name, schema.table
                )),
                Some(serde_json::json!({
                    "table": schema.table,
                    "field": draft.field_name,
                })),
            )
        })?;

        let rust_enum_name = request
            .enum_name_overrides
            .get(&draft.field_name)
            .cloned()
            .unwrap_or_else(|| draft.rust_enum_name.clone());
        let enum_name_source = if request.enum_name_overrides.contains_key(&draft.field_name) {
            EntityEnumNameSource::Override
        } else {
            EntityEnumNameSource::Draft
        };

        let definition_action = if existing_enums.contains(&rust_enum_name) {
            EntityEnumDefinitionAction::ReuseExisting
        } else {
            EntityEnumDefinitionAction::Create
        };

        let variant_overrides = request.variant_name_overrides.get(&draft.field_name);
        let variants = build_variant_plans(draft, variant_overrides);
        let option_value_kind = draft
            .options
            .first()
            .map(|option| option.value_kind)
            .unwrap_or(EnumOptionValueKind::String);
        let (sea_orm_db_type, sea_orm_enum_name) =
            infer_sea_orm_db_type(draft.source, column, option_value_kind);
        let to_rust_type = if field.nullable_entity {
            format!("Option<{rust_enum_name}>")
        } else {
            rust_enum_name.clone()
        };

        plans.push(EntityEnumFieldPlan {
            field_name: draft.field_name.clone(),
            field_label: draft.field_label.clone(),
            source: draft.source,
            definition_action,
            from_rust_type: field.type_info.rust_entity.clone(),
            to_rust_type,
            rust_enum_name,
            enum_name_source,
            sea_orm_rs_type: draft.rust_rs_type.clone(),
            sea_orm_db_type,
            sea_orm_enum_name,
            needs_review: variants
                .iter()
                .any(|variant| variant.name_source == EntityEnumVariantNameSource::ValueFallback),
            variants,
        });
    }

    if let Some(selected) = selected {
        let planned = plans
            .iter()
            .map(|plan| plan.field_name.clone())
            .collect::<BTreeSet<_>>();
        let missing = selected.difference(&planned).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(invalid_params_error(
                "entity_enum_upgrade_field_not_supported",
                "Requested field is not a pending enum upgrade candidate",
                Some("Only fields with semantic enum drafts can be upgraded."),
                Some(format!(
                    "unsupported fields for table `{}`: {}",
                    schema.table,
                    missing.join(", ")
                )),
                Some(serde_json::json!({
                    "table": schema.table,
                    "fields": missing,
                })),
            ));
        }
    }

    Ok(plans)
}

fn build_plan_warnings(
    request: &EntityEnumUpgradeRequest,
    planned_fields: &[EntityEnumFieldPlan],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if planned_fields.is_empty() {
        warnings.push("No pending enum upgrades detected for this entity file.".to_string());
    }
    for field in planned_fields {
        if field.needs_review {
            warnings.push(format!(
                "Field `{}` uses fallback Rust variant names for one or more enum options; consider variant_name_overrides before apply.",
                field.field_name
            ));
        }
        if field.definition_action == EntityEnumDefinitionAction::ReuseExisting {
            warnings.push(format!(
                "Field `{}` will reuse existing enum `{}` instead of inserting a new definition.",
                field.field_name, field.rust_enum_name
            ));
        }
    }
    for field in request.enum_name_overrides.keys() {
        if !planned_fields.iter().any(|plan| &plan.field_name == field) {
            warnings.push(format!(
                "enum_name_overrides for field `{field}` was ignored because that field is not part of the current upgrade plan."
            ));
        }
    }
    warnings
}

fn build_variant_plans(
    draft: &EnumDraftSpec,
    overrides: Option<&BTreeMap<String, String>>,
) -> Vec<EntityEnumVariantPlan> {
    let mut used = BTreeSet::new();
    draft
        .options
        .iter()
        .enumerate()
        .map(|(idx, option)| {
            let override_name = overrides
                .and_then(|variants| variants.get(&option.value))
                .cloned();
            let (candidate_name, source) = if let Some(name) = override_name {
                (name, EntityEnumVariantNameSource::Override)
            } else if let Some(name) = label_ascii_identifier(&option.label) {
                (name, EntityEnumVariantNameSource::LabelAscii)
            } else if let Some(name) = common_label_identifier(&option.label) {
                (name, EntityEnumVariantNameSource::CommonLabel)
            } else {
                (
                    fallback_variant_name(&option.value, idx),
                    EntityEnumVariantNameSource::ValueFallback,
                )
            };
            let rust_variant_name = dedupe_variant_name(candidate_name, &mut used);

            EntityEnumVariantPlan {
                label: option.label.clone(),
                value: option.value.clone(),
                value_kind: option.value_kind,
                rust_variant_name,
                name_source: source,
            }
        })
        .collect()
}

fn dedupe_variant_name(candidate: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(candidate.clone()) {
        return candidate;
    }

    let mut index = 2usize;
    loop {
        let value = format!("{candidate}{index}");
        if used.insert(value.clone()) {
            return value;
        }
        index += 1;
    }
}

fn label_ascii_identifier(label: &str) -> Option<String> {
    let segments = label
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }

    let mut result = String::new();
    for segment in segments {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            if result.is_empty() && first.is_ascii_digit() {
                result.push_str("Value");
            }
            result.push(first.to_ascii_uppercase());
            result.push_str(chars.as_str());
        }
    }

    (!result.is_empty()).then_some(result)
}

fn common_label_identifier(label: &str) -> Option<String> {
    match label.trim() {
        "未知" => Some("Unknown".to_string()),
        "男" => Some("Male".to_string()),
        "女" => Some("Female".to_string()),
        "启用" => Some("Enabled".to_string()),
        "正常" => Some("Normal".to_string()),
        "禁用" => Some("Disabled".to_string()),
        "停用" => Some("Disabled".to_string()),
        "注销" => Some("Cancelled".to_string()),
        "成功" => Some("Success".to_string()),
        "失败" => Some("Failed".to_string()),
        "异常" => Some("Exception".to_string()),
        "草稿" => Some("Draft".to_string()),
        "已发布" => Some("Published".to_string()),
        "已归档" => Some("Archived".to_string()),
        "菜单" => Some("Menu".to_string()),
        "按钮" | "按钮权限" => Some("Button".to_string()),
        "其他" => Some("Other".to_string()),
        "新增" => Some("Create".to_string()),
        "修改" => Some("Update".to_string()),
        "删除" => Some("Delete".to_string()),
        "查询" => Some("Query".to_string()),
        "导出" => Some("Export".to_string()),
        "导入" => Some("Import".to_string()),
        "授权" | "认证" => Some("Auth".to_string()),
        _ => None,
    }
}

fn fallback_variant_name(value: &str, index: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return format!("Variant{}", index + 1);
    }

    let mut output = String::from("Value");
    for ch in trimmed.chars() {
        match ch {
            '-' => output.push_str("Minus"),
            '0'..='9' => output.push(ch),
            'a'..='z' | 'A'..='Z' => {
                if output.ends_with(|char: char| char.is_ascii_digit()) {
                    output.push('_');
                }
                output.push(ch.to_ascii_uppercase());
            }
            _ => {}
        }
    }

    if output == "Value" {
        format!("Variant{}", index + 1)
    } else {
        output
    }
}

fn infer_sea_orm_db_type(
    source: EnumSemanticSource,
    column: &TableColumnSchema,
    value_kind: EnumOptionValueKind,
) -> (String, Option<String>) {
    match value_kind {
        EnumOptionValueKind::Number => (numeric_db_type(&column.pg_type), None),
        EnumOptionValueKind::String => {
            if source == EnumSemanticSource::NativeDbEnum {
                ("Enum".to_string(), Some(column.pg_type.clone()))
            } else {
                ("String(StringLen::None)".to_string(), None)
            }
        }
    }
}

fn numeric_db_type(pg_type: &str) -> String {
    match pg_type.trim().to_ascii_lowercase().as_str() {
        "smallint" | "smallserial" | "int2" => "SmallInteger".to_string(),
        "bigint" | "bigserial" | "int8" => "BigInteger".to_string(),
        _ => "Integer".to_string(),
    }
}

fn apply_ast_upgrade(
    file: &mut File,
    planned_fields: &[EntityEnumFieldPlan],
) -> Result<(), McpError> {
    let need_json_schema = !planned_fields.is_empty();
    let need_serde_repr = planned_fields.iter().any(|field| {
        field
            .variants
            .first()
            .is_some_and(|variant| variant.value_kind == EnumOptionValueKind::Number)
    });

    if need_json_schema {
        ensure_use_item(file, "use schemars::JsonSchema;")?;
    }
    if need_serde_repr {
        ensure_use_item(file, "use serde_repr::{Deserialize_repr, Serialize_repr};")?;
    }

    let model = file
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == "Model" => Some(item),
            _ => None,
        })
        .ok_or_else(|| {
            internal_error(
                "entity_model_not_found",
                "Entity model not found",
                Some("Run generate_entity_from_table first so the entity contains `pub struct Model`."),
                Some("failed to find `pub struct Model` in entity source".to_string()),
                None,
            )
        })?;
    apply_field_type_updates(model, planned_fields)?;

    let model_index = file
        .items
        .iter()
        .position(|item| matches!(item, Item::Struct(item) if item.ident == "Model"))
        .expect("model index must exist");

    let mut insert_offset = 0usize;
    for field in planned_fields {
        if field.definition_action == EntityEnumDefinitionAction::ReuseExisting {
            continue;
        }
        let enum_item = render_enum_item(field)?;
        file.items
            .insert(model_index + insert_offset, Item::Enum(enum_item));
        insert_offset += 1;
    }

    Ok(())
}

fn apply_field_type_updates(
    model: &mut ItemStruct,
    planned_fields: &[EntityEnumFieldPlan],
) -> Result<(), McpError> {
    let Fields::Named(fields) = &mut model.fields else {
        return Err(internal_error(
            "entity_model_fields_invalid",
            "Entity model fields are invalid",
            Some("Entity `Model` must use named fields."),
            Some("entity `Model` is not a named-field struct".to_string()),
            None,
        ));
    };

    let plan_map = planned_fields
        .iter()
        .map(|field| (field.field_name.as_str(), field))
        .collect::<BTreeMap<_, _>>();

    for field in &mut fields.named {
        let Some(ident) = &field.ident else {
            continue;
        };
        let Some(plan) = plan_map.get(ident.to_string().as_str()) else {
            continue;
        };
        field.ty = syn::parse_str::<Type>(&plan.to_rust_type).map_err(|error| {
            internal_error(
                "render_target_type_failed",
                "Failed to render target Rust type",
                Some("Check generated enum names and nullable field handling."),
                Some(format!(
                    "failed to parse target type `{}` for field `{}`: {error}",
                    plan.to_rust_type, plan.field_name
                )),
                None,
            )
        })?;
    }

    Ok(())
}

fn ensure_use_item(file: &mut File, use_source: &str) -> Result<(), McpError> {
    let needle = use_source.replace(' ', "");
    if file.items.iter().any(|item| {
        matches!(item, Item::Use(item) if item.to_token_stream().to_string().replace(' ', "") == needle)
    }) {
        return Ok(());
    }

    let use_item = syn::parse_str::<Item>(use_source).map_err(|error| {
        internal_error(
            "parse_use_item_failed",
            "Failed to parse generated use item",
            None,
            Some(format!("failed to parse `{use_source}`: {error}")),
            None,
        )
    })?;

    let insert_index = file
        .items
        .iter()
        .rposition(|item| matches!(item, Item::Use(_)))
        .map(|index| index + 1)
        .unwrap_or(0);
    file.items.insert(insert_index, use_item);
    Ok(())
}

fn render_enum_item(field: &EntityEnumFieldPlan) -> Result<syn::ItemEnum, McpError> {
    let is_numeric = field
        .variants
        .first()
        .is_some_and(|variant| variant.value_kind == EnumOptionValueKind::Number);

    let derives = if is_numeric {
        r#"Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize_repr, Deserialize_repr, JsonSchema"#
    } else {
        r#"Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, JsonSchema"#
    };

    let sea_orm_attr = match &field.sea_orm_enum_name {
        Some(enum_name) => format!(
            r#"#[sea_orm(rs_type = "{}", db_type = "{}", enum_name = "{}")]"#,
            field.sea_orm_rs_type, field.sea_orm_db_type, enum_name
        ),
        None => format!(
            r#"#[sea_orm(rs_type = "{}", db_type = "{}")]"#,
            field.sea_orm_rs_type, field.sea_orm_db_type
        ),
    };

    let repr_attr = if is_numeric {
        format!("#[repr({})]", field.sea_orm_rs_type)
    } else {
        String::new()
    };

    let variants = field
        .variants
        .iter()
        .map(|variant| {
            let label = sanitize_doc_line(&variant.label);
            match variant.value_kind {
                EnumOptionValueKind::Number => format!(
                    "    /// {label}\n    #[sea_orm(num_value = {value})]\n    {name} = {value},",
                    value = variant.value,
                    name = variant.rust_variant_name,
                ),
                EnumOptionValueKind::String => format!(
                    "    /// {label}\n    #[sea_orm(string_value = {value:?})]\n    {name},",
                    value = variant.value,
                    name = variant.rust_variant_name,
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let enum_source = format!(
        "/// {label}\n#[derive({derives})]\n{sea_orm_attr}\n{repr_attr}\npub enum {enum_name} {{\n{variants}\n}}",
        label = sanitize_doc_line(&field.field_label),
        enum_name = field.rust_enum_name,
    );

    syn::parse_str::<syn::ItemEnum>(&enum_source).map_err(|error| {
        internal_error(
            "render_enum_definition_failed",
            "Failed to render enum definition",
            Some("Check generated variant names and enum metadata."),
            Some(format!(
                "failed to parse generated enum `{}`: {error}",
                field.rust_enum_name
            )),
            None,
        )
    })
}

fn sanitize_doc_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn ensure_valid_rust_ident(value: &str, label: &str) -> Result<(), McpError> {
    syn::parse_str::<syn::Ident>(value).map_err(|error| {
        invalid_params_error(
            "invalid_rust_identifier",
            "Invalid Rust identifier",
            Some("Use a valid Rust PascalCase identifier for enum or variant names."),
            Some(format!("{label} `{value}` is invalid: {error}")),
            Some(serde_json::json!({ "label": label, "value": value })),
        )
    })?;
    Ok(())
}

async fn format_rust_source(source: &str) -> Result<String, McpError> {
    let temp_file = temp_rustfmt_file_path()?;
    tokio::fs::write(&temp_file, source)
        .await
        .map_err(|error| {
            io_error(
                format!("write temporary rustfmt file `{}`", temp_file.display()),
                error,
            )
        })?;

    let result = try_rustfmt_file(&temp_file).await;
    let formatted = tokio::fs::read_to_string(&temp_file)
        .await
        .map_err(|error| {
            io_error(
                format!("read formatted temporary file `{}`", temp_file.display()),
                error,
            )
        })?;
    let _ = tokio::fs::remove_file(&temp_file).await;
    result?;
    Ok(formatted)
}

async fn try_rustfmt_file(path: &Path) -> Result<(), McpError> {
    let output = match Command::new("rustfmt").arg(path).output().await {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(io_error(
                format!("run rustfmt on `{}`", path.display()),
                error,
            ));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    Err(internal_error(
        "rustfmt_failed",
        "Rust formatting failed",
        Some("Check the generated entity syntax before retrying."),
        Some(format!(
            "rustfmt failed for `{}` (status {}): stderr=`{}` stdout=`{}`",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim(),
        )),
        None,
    ))
}

fn temp_rustfmt_file_path() -> Result<PathBuf, McpError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            internal_error(
                "timestamp_failed",
                "Timestamp failed",
                None,
                Some(error.to_string()),
                None,
            )
        })?
        .as_nanos();
    let seq = TEMP_RUSTFMT_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(std::env::temp_dir().join(format!(
        "summer-mcp-entity-enum-upgrade-{timestamp}-{}-{seq}.rs",
        std::process::id(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::validation::ValidationStatus;

    fn sample_schema() -> TableSchema {
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
                    comment: Some("主键".to_string()),
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
                    comment: Some("状态：1-启用 2-禁用 3-注销".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "contact_gender".to_string(),
                    pg_type: "smallint".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("联系人性别：0-未知 1-男 2-女".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        }
    }

    fn sample_entity_source() -> &'static str {
        r#"//! generated

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub status: i16,
    pub contact_gender: Option<i16>,
}

impl ActiveModelBehavior for ActiveModel {}
"#
    }

    #[tokio::test]
    async fn plan_builds_upgrade_preview_from_comment_backed_fields() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-entity-enum-plan-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let entity_dir = root.join("crates/model/src/entity");
        std::fs::create_dir_all(&entity_dir).unwrap();
        std::fs::write(entity_dir.join("sys_user.rs"), sample_entity_source()).unwrap();

        let upgrader = EntityEnumUpgrader::with_workspace_root(root.clone());
        let preview = upgrader
            .plan(EntityEnumUpgradeRequest {
                schema: sample_schema(),
                route_base: None,
                output_dir: None,
                fields: None,
                enum_name_overrides: BTreeMap::new(),
                variant_name_overrides: BTreeMap::new(),
            })
            .await
            .unwrap();

        assert!(preview.changed);
        assert_eq!(preview.plan.fields.len(), 2);
        assert_eq!(preview.plan.fields[0].rust_enum_name, "UserStatus");
        assert_eq!(preview.plan.fields[1].rust_enum_name, "ContactGender");
        assert!(preview.rendered_source.contains("pub enum UserStatus"));
        assert!(preview.rendered_source.contains("pub status: UserStatus"));
        assert!(
            preview
                .rendered_source
                .contains("pub contact_gender: Option<ContactGender>")
        );
        assert!(preview.rendered_source.contains("Serialize_repr"));
        assert!(preview.rendered_source.contains("JsonSchema"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn apply_writes_upgraded_entity_and_validates() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-entity-enum-apply-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let entity_dir = root.join("generated/entity");
        std::fs::create_dir_all(&entity_dir).unwrap();
        std::fs::write(entity_dir.join("sys_user.rs"), sample_entity_source()).unwrap();

        let upgrader = EntityEnumUpgrader::with_workspace_root(root.clone());
        let result = upgrader
            .apply(EntityEnumUpgradeRequest {
                schema: sample_schema(),
                route_base: None,
                output_dir: Some(entity_dir.display().to_string()),
                fields: Some(vec!["status".to_string()]),
                enum_name_overrides: BTreeMap::new(),
                variant_name_overrides: BTreeMap::new(),
            })
            .await
            .unwrap();

        let source = std::fs::read_to_string(entity_dir.join("sys_user.rs")).unwrap();
        assert!(source.contains("pub enum UserStatus"));
        assert!(source.contains("pub status: UserStatus"));
        assert!(source.contains("pub contact_gender: Option<i16>"));
        assert_eq!(result.validation.status, ValidationStatus::Passed);
        assert!(
            result
                .validation
                .checks
                .iter()
                .any(|check| check.name == "rust_workspace_check"
                    && check.status == ValidationStatus::Skipped)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn variant_plan_uses_common_label_and_fallback_names() {
        let draft = EnumDraftSpec {
            field_name: "status".to_string(),
            field_label: "状态".to_string(),
            source: EnumSemanticSource::ColumnComment,
            rust_enum_name: "UserStatus".to_string(),
            rust_rs_type: "i16".to_string(),
            existing_entity_path: None,
            dict_type: "user_status".to_string(),
            dict_name: "用户状态".to_string(),
            options: vec![
                super::super::enum_semantics::EnumDraftOptionSpec {
                    label: "启用".to_string(),
                    value: "1".to_string(),
                    value_kind: EnumOptionValueKind::Number,
                },
                super::super::enum_semantics::EnumDraftOptionSpec {
                    label: "待人工确认".to_string(),
                    value: "9".to_string(),
                    value_kind: EnumOptionValueKind::Number,
                },
            ],
        };

        let plans = build_variant_plans(&draft, None);
        assert_eq!(plans[0].rust_variant_name, "Enabled");
        assert_eq!(
            plans[1].name_source,
            EntityEnumVariantNameSource::ValueFallback
        );
    }
}
