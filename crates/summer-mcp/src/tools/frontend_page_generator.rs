use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    table_tools::schema::{TableColumnSchema, TableSchema, ensure_valid_identifier},
    tools::{
        frontend_target::FrontendTargetPreset,
        generation_context::{
            CrudGenerationContext, CrudGenerationContextBuilder, CrudNamingContext,
            EnumOptionContext, FieldGenerationContext, FieldTypeContext, FieldValueKind,
            TableGenerationContext,
        },
        support::{default_route_base, io_error, workspace_root},
        template_renderer::{EmbeddedTemplate, TemplateRenderer},
    },
};

const MODEL_ENTITY_DIR: &str = "crates/model/src/entity";

const FRONTEND_PAGE_INDEX_TEMPLATE_NAME: &str = "frontend/page/index.vue.j2";
const FRONTEND_PAGE_SEARCH_TEMPLATE_NAME: &str = "frontend/page/search.vue.j2";
const FRONTEND_PAGE_DIALOG_TEMPLATE_NAME: &str = "frontend/page/dialog.vue.j2";

const FRONTEND_PAGE_TEMPLATES: [EmbeddedTemplate; 3] = [
    EmbeddedTemplate {
        name: FRONTEND_PAGE_INDEX_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/page/index.vue.j2"),
    },
    EmbeddedTemplate {
        name: FRONTEND_PAGE_SEARCH_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/page/search.vue.j2"),
    },
    EmbeddedTemplate {
        name: FRONTEND_PAGE_DIALOG_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/page/dialog.vue.j2"),
    },
];

#[derive(Debug, Clone)]
pub struct FrontendPageGenerator {
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GenerateFrontendPageRequest {
    pub schema: TableSchema,
    pub overwrite: bool,
    pub route_base: Option<String>,
    pub output_dir: Option<String>,
    pub target_preset: FrontendTargetPreset,
    pub api_import_path: Option<String>,
    pub api_namespace: Option<String>,
    pub api_list_item_type_name: Option<String>,
    pub api_detail_type_name: Option<String>,
    pub dict_bindings: BTreeMap<String, String>,
    pub field_hints: BTreeMap<String, FrontendFieldUiHint>,
    pub search_fields: Option<Vec<String>>,
    pub table_fields: Option<Vec<String>>,
    pub form_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct GenerateFrontendPageResult {
    pub table: String,
    pub route_base: String,
    pub api_import_path: String,
    pub api_namespace: String,
    pub page_dir: PathBuf,
    pub index_file: PathBuf,
    pub search_file: PathBuf,
    pub dialog_file: PathBuf,
    pub required_dict_types: Vec<String>,
}

#[derive(Debug, Clone)]
struct GeneratedPaths {
    entity_file: PathBuf,
    page_dir: PathBuf,
    index_file: PathBuf,
    search_file: PathBuf,
    dialog_file: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendPageTemplateContext {
    table: TableGenerationContext,
    names: CrudNamingContext,
    primary_key: FieldGenerationContext,
    page: FrontendPageNamingContext,
    api: FrontendPageApiContext,
    flags: FrontendPageFlags,
    search_fields: Vec<FrontendPageFieldContext>,
    table_fields: Vec<FrontendPageFieldContext>,
    form_fields: Vec<FrontendPageFieldContext>,
    required_dict_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendPageNamingContext {
    folder_name: String,
    component_name: String,
    search_component_name: String,
    dialog_component_name: String,
    search_file_name: String,
    dialog_file_name: String,
    list_item_alias: String,
    current_item_ref_name: String,
    item_prop_name: String,
    item_prop_attr_name: String,
    delete_display_field: String,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendPageApiContext {
    import_path: String,
    namespace: String,
    list_item_type_name: String,
    detail_type_name: String,
    search_params_type_name: String,
    create_params_type_name: String,
    update_params_type_name: String,
    fetch_list_fn: String,
    fetch_detail_fn: String,
    fetch_create_fn: String,
    fetch_update_fn: String,
    fetch_delete_fn: String,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendPageFlags {
    uses_table_dict: bool,
    uses_search_dict: bool,
    uses_form_dict: bool,
    uses_table_tag: bool,
    uses_table_image: bool,
    uses_form_upload: bool,
    uses_avatar_placeholder: bool,
    uses_range_search: bool,
    has_search_fields: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendPageFieldContext {
    name: String,
    camel_name: String,
    pascal_name: String,
    label: String,
    semantic_kind: FieldSemanticKind,
    type_info: FieldTypeContext,
    dict_type: Option<String>,
    local_options_literal: Option<String>,
    search_widget: SearchWidgetKind,
    search_visible: bool,
    search_model_key: String,
    search_exclude_param: bool,
    search_param_start: Option<String>,
    search_param_end: Option<String>,
    search_value_format: Option<String>,
    form_widget: FormWidgetKind,
    form_visibility: FormFieldVisibility,
    table_display: TableDisplayKind,
    table_width: Option<u16>,
    table_min_width: Option<u16>,
    table_sortable: bool,
    table_overflow: bool,
    form_required: bool,
    form_default_value: String,
    form_model_type: String,
    create_submit_value: String,
    update_submit_value: String,
    search_placeholder: String,
    form_placeholder: String,
    form_upload_accept: Option<String>,
    form_upload_button_text: Option<String>,
    form_rule_trigger: String,
    bool_true_label: String,
    bool_false_label: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendFieldUiHint {
    pub semantic: Option<FieldSemanticKind>,
    pub search_visible: Option<bool>,
    pub search_widget: Option<SearchWidgetKind>,
    pub form_widget: Option<FormWidgetKind>,
    pub table_display: Option<TableDisplayKind>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchWidgetKind {
    Input,
    Number,
    Select,
    RadioGroup,
    Date,
    DateRange,
    Time,
    DateTime,
    DateTimeRange,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FormWidgetKind {
    Input,
    Password,
    Textarea,
    Switch,
    Select,
    InputNumber,
    Date,
    Time,
    DateTime,
    ImageUpload,
    FileUpload,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FormFieldVisibility {
    Always,
    AddOnly,
    EditOnly,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TableDisplayKind {
    Text,
    BooleanTag,
    DictTag,
    LocalTag,
    Image,
    Link,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FieldSemanticKind {
    Plain,
    Password,
    Email,
    Phone,
    Url,
    Avatar,
    Image,
    File,
    Icon,
    RichText,
}

impl FrontendPageGenerator {
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
        request: GenerateFrontendPageRequest,
    ) -> Result<GenerateFrontendPageResult, McpError> {
        self.validate_request(&request)?;

        let paths = self.build_paths(
            &request.schema.table,
            request.route_base.as_deref(),
            request.output_dir.as_deref(),
            request.target_preset,
        )?;
        for path in [&paths.index_file, &paths.search_file, &paths.dialog_file] {
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
        let crud_context = CrudGenerationContextBuilder::build_from_entity_source(
            request.schema.clone(),
            request.route_base.clone(),
            &entity_source,
        )?;
        let template_context = build_template_context(request, crud_context)?;
        let renderer = TemplateRenderer::new(&FRONTEND_PAGE_TEMPLATES)?;

        let index_source = renderer.render(FRONTEND_PAGE_INDEX_TEMPLATE_NAME, &template_context)?;
        let search_source =
            renderer.render(FRONTEND_PAGE_SEARCH_TEMPLATE_NAME, &template_context)?;
        let dialog_source =
            renderer.render(FRONTEND_PAGE_DIALOG_TEMPLATE_NAME, &template_context)?;

        tokio::fs::write(&paths.index_file, index_source)
            .await
            .map_err(|error| {
                io_error(
                    format!("write frontend page file `{}`", paths.index_file.display()),
                    error,
                )
            })?;
        tokio::fs::write(&paths.search_file, search_source)
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "write frontend search file `{}`",
                        paths.search_file.display()
                    ),
                    error,
                )
            })?;
        tokio::fs::write(&paths.dialog_file, dialog_source)
            .await
            .map_err(|error| {
                io_error(
                    format!(
                        "write frontend dialog file `{}`",
                        paths.dialog_file.display()
                    ),
                    error,
                )
            })?;

        Ok(GenerateFrontendPageResult {
            table: template_context.table.name.clone(),
            route_base: template_context.table.route_base.clone(),
            api_import_path: template_context.api.import_path.clone(),
            api_namespace: template_context.api.namespace.clone(),
            page_dir: paths.page_dir,
            index_file: paths.index_file,
            search_file: paths.search_file,
            dialog_file: paths.dialog_file,
            required_dict_types: template_context.required_dict_types,
        })
    }

    fn validate_request(&self, request: &GenerateFrontendPageRequest) -> Result<(), McpError> {
        ensure_valid_identifier(&request.schema.table, "table")?;
        if let Some(route_base) = &request.route_base {
            ensure_valid_identifier(route_base, "route_base")?;
        }
        if let Some(api_namespace) = &request.api_namespace {
            if api_namespace.trim().is_empty() {
                return Err(McpError::invalid_params(
                    "api_namespace cannot be empty",
                    None,
                ));
            }
        }
        for (label, value) in [
            (
                "api_list_item_type_name",
                request.api_list_item_type_name.as_deref(),
            ),
            (
                "api_detail_type_name",
                request.api_detail_type_name.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                if value.trim().is_empty() {
                    return Err(McpError::invalid_params(
                        format!("{label} cannot be empty"),
                        None,
                    ));
                }
            }
        }
        for (field, dict_type) in &request.dict_bindings {
            ensure_valid_identifier(field, "dict_bindings field")?;
            if dict_type.trim().is_empty() {
                return Err(McpError::invalid_params(
                    format!("dict binding for field `{field}` cannot be empty"),
                    None,
                ));
            }
        }
        for field in request.field_hints.keys() {
            ensure_valid_identifier(field, "field_hints field")?;
        }
        for (label, fields) in [
            ("search_fields", request.search_fields.as_deref()),
            ("table_fields", request.table_fields.as_deref()),
            ("form_fields", request.form_fields.as_deref()),
        ] {
            if let Some(fields) = fields {
                for field in fields {
                    ensure_valid_identifier(field, label)?;
                }
            }
        }
        Ok(())
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
        let base_dir = target_preset.resolve_page_output_dir(&self.workspace_root, output_dir)?;
        let page_dir = base_dir.join(&file_stem);
        let modules_dir = page_dir.join("modules");
        Ok(GeneratedPaths {
            entity_file: self
                .workspace_root
                .join(MODEL_ENTITY_DIR)
                .join(format!("{table}.rs")),
            page_dir: page_dir.clone(),
            index_file: page_dir.join("index.vue"),
            search_file: modules_dir.join(format!("{file_stem}-search.vue")),
            dialog_file: modules_dir.join(format!("{file_stem}-dialog.vue")),
        })
    }

    async fn ensure_parent_dirs(&self, paths: &GeneratedPaths) -> Result<(), McpError> {
        for directory in [
            Some(paths.page_dir.as_path()),
            paths.search_file.parent(),
            paths.dialog_file.parent(),
        ]
        .into_iter()
        .flatten()
        {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|error| {
                    io_error(
                        format!(
                            "create frontend generator output directory `{}`",
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

fn build_template_context(
    request: GenerateFrontendPageRequest,
    crud_context: CrudGenerationContext,
) -> Result<FrontendPageTemplateContext, McpError> {
    let table = crud_context.table.clone();
    let names = crud_context.names.clone();
    let primary_key = crud_context.primary_key.clone();
    let api = build_api_context(&crud_context, &request);
    let field_map = build_field_map(&request.schema, &crud_context, &request, &api)?;

    let allowed_search_fields = crud_context
        .query_fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<BTreeSet<_>>();
    let search_fields = select_field_contexts(
        "search_fields",
        &field_map,
        request.search_fields.as_deref(),
        &allowed_search_fields,
        default_search_field_names(&crud_context, &field_map),
    )?;
    ensure_unique_search_model_keys(&search_fields)?;
    let allowed_table_fields = crud_context
        .read_fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<BTreeSet<_>>();
    let table_fields = select_field_contexts(
        "table_fields",
        &field_map,
        request.table_fields.as_deref(),
        &allowed_table_fields,
        default_table_field_names(&crud_context),
    )?;
    if table_fields.is_empty() {
        return Err(McpError::invalid_params(
            format!(
                "table `{}` does not have readable fields suitable for a frontend page",
                table.name
            ),
            None,
        ));
    }

    let allowed_form_fields = default_form_field_names(&crud_context)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let form_fields = select_field_contexts(
        "form_fields",
        &field_map,
        request.form_fields.as_deref(),
        &allowed_form_fields,
        allowed_form_fields.iter().cloned().collect(),
    )?;
    if form_fields.is_empty() {
        return Err(McpError::invalid_params(
            format!(
                "table `{}` does not have create/update fields suitable for a frontend dialog",
                table.name
            ),
            None,
        ));
    }

    let required_dict_types =
        collect_required_dict_types([&search_fields, &table_fields, &form_fields]);
    let page = build_page_naming_context(&crud_context, &table_fields);
    let flags = FrontendPageFlags {
        uses_table_dict: table_fields.iter().any(|field| field.dict_type.is_some()),
        uses_search_dict: search_fields.iter().any(|field| field.dict_type.is_some()),
        uses_form_dict: form_fields.iter().any(|field| field.dict_type.is_some()),
        uses_table_tag: table_fields.iter().any(|field| {
            matches!(
                field.table_display,
                TableDisplayKind::BooleanTag
                    | TableDisplayKind::DictTag
                    | TableDisplayKind::LocalTag
            )
        }),
        uses_table_image: table_fields
            .iter()
            .any(|field| matches!(field.table_display, TableDisplayKind::Image)),
        uses_form_upload: form_fields.iter().any(|field| {
            matches!(
                field.form_widget,
                FormWidgetKind::ImageUpload | FormWidgetKind::FileUpload
            )
        }),
        uses_avatar_placeholder: table_fields
            .iter()
            .chain(form_fields.iter())
            .any(|field| field.semantic_kind == FieldSemanticKind::Avatar),
        uses_range_search: search_fields.iter().any(|field| field.search_exclude_param),
        has_search_fields: !search_fields.is_empty(),
    };

    Ok(FrontendPageTemplateContext {
        table,
        names,
        primary_key,
        page,
        api,
        flags,
        search_fields,
        table_fields,
        form_fields,
        required_dict_types,
    })
}

fn build_api_context(
    crud_context: &CrudGenerationContext,
    request: &GenerateFrontendPageRequest,
) -> FrontendPageApiContext {
    FrontendPageApiContext::from_generated_contract(crud_context).with_request_overrides(request)
}

impl FrontendPageApiContext {
    fn from_generated_contract(crud_context: &CrudGenerationContext) -> Self {
        let route_file_stem = crud_context.table.file_stem.clone();
        let api_function_base = crud_context.names.api_function_base.clone();
        let resource_pascal = crud_context.names.resource_pascal.clone();

        Self {
            import_path: format!("@/api/{route_file_stem}"),
            namespace: crud_context.names.ts_namespace.clone(),
            list_item_type_name: format!("{resource_pascal}Vo"),
            detail_type_name: format!("{resource_pascal}DetailVo"),
            search_params_type_name: format!("{resource_pascal}SearchParams"),
            create_params_type_name: format!("Create{resource_pascal}Params"),
            update_params_type_name: format!("Update{resource_pascal}Params"),
            fetch_list_fn: format!("fetchGet{api_function_base}List"),
            fetch_detail_fn: format!("fetchGet{api_function_base}Detail"),
            fetch_create_fn: format!("fetchCreate{api_function_base}"),
            fetch_update_fn: format!("fetchUpdate{api_function_base}"),
            fetch_delete_fn: format!("fetchDelete{api_function_base}"),
        }
    }

    fn with_request_overrides(mut self, request: &GenerateFrontendPageRequest) -> Self {
        if let Some(import_path) = &request.api_import_path {
            self.import_path = import_path.clone();
        }
        if let Some(namespace) = &request.api_namespace {
            self.namespace = namespace.clone();
        }
        if let Some(list_item_type_name) = &request.api_list_item_type_name {
            self.list_item_type_name = list_item_type_name.clone();
        }
        if let Some(detail_type_name) = &request.api_detail_type_name {
            self.detail_type_name = detail_type_name.clone();
        }
        self
    }
}

fn build_field_map(
    schema: &TableSchema,
    crud_context: &CrudGenerationContext,
    request: &GenerateFrontendPageRequest,
    api: &FrontendPageApiContext,
) -> Result<BTreeMap<String, FrontendPageFieldContext>, McpError> {
    let columns = schema
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let create_fields = crud_context
        .create_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let update_fields = crud_context
        .update_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();

    let known_fields = crud_context
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    for field in request.dict_bindings.keys() {
        if !known_fields.contains(field.as_str()) {
            return Err(McpError::invalid_params(
                format!(
                    "dict binding field `{field}` does not exist on table `{}`",
                    schema.table
                ),
                None,
            ));
        }
    }
    for field in request.field_hints.keys() {
        if !known_fields.contains(field.as_str()) {
            return Err(McpError::invalid_params(
                format!(
                    "field hint `{field}` does not exist on table `{}`",
                    schema.table
                ),
                None,
            ));
        }
    }

    crud_context
        .fields
        .iter()
        .map(|field| {
            let column = columns.get(field.name.as_str()).copied().ok_or_else(|| {
                McpError::internal_error(
                    format!(
                        "schema column `{}` was not found when building frontend context",
                        field.name
                    ),
                    None,
                )
            })?;
            let dict_type = request.dict_bindings.get(&field.name).cloned();
            let field_hint = request
                .field_hints
                .get(&field.name)
                .cloned()
                .unwrap_or_default();
            Ok((
                field.name.clone(),
                build_frontend_field_context(
                    field,
                    column,
                    create_fields.contains(field.name.as_str()),
                    update_fields.contains(field.name.as_str()),
                    dict_type,
                    field_hint,
                    api,
                ),
            ))
        })
        .collect()
}

fn build_frontend_field_context(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
    create_enabled: bool,
    update_enabled: bool,
    dict_type: Option<String>,
    field_hint: FrontendFieldUiHint,
    api: &FrontendPageApiContext,
) -> FrontendPageFieldContext {
    let semantic_kind = field_hint
        .semantic
        .unwrap_or_else(|| infer_field_semantic(field, column));
    let local_options = local_options(field, column);
    let has_enum_options = !local_options.is_empty();
    let has_select_options = dict_type.is_some()
        || has_enum_options
        || field.type_info.value_kind == FieldValueKind::Boolean;
    let uses_range_search = search_should_use_range(field, &semantic_kind);
    let inferred_search_widget = match field.type_info.value_kind {
        _ if has_enum_options && is_gender_like_field(field) => SearchWidgetKind::RadioGroup,
        FieldValueKind::Date if uses_range_search => SearchWidgetKind::DateRange,
        FieldValueKind::Date => SearchWidgetKind::Date,
        FieldValueKind::Time => SearchWidgetKind::Time,
        FieldValueKind::DateTime if uses_range_search => SearchWidgetKind::DateTimeRange,
        FieldValueKind::DateTime => SearchWidgetKind::DateTime,
        FieldValueKind::Integer | FieldValueKind::Float if !has_select_options => {
            SearchWidgetKind::Number
        }
        _ if has_select_options => SearchWidgetKind::Select,
        _ => SearchWidgetKind::Input,
    };
    let search_widget = field_hint.search_widget.unwrap_or(inferred_search_widget);
    let inferred_form_widget = match field.type_info.value_kind {
        _ if semantic_kind == FieldSemanticKind::Password => FormWidgetKind::Password,
        _ if matches!(
            semantic_kind,
            FieldSemanticKind::Avatar | FieldSemanticKind::Image
        ) =>
        {
            FormWidgetKind::ImageUpload
        }
        _ if semantic_kind == FieldSemanticKind::File => FormWidgetKind::FileUpload,
        FieldValueKind::Boolean => FormWidgetKind::Switch,
        FieldValueKind::Integer | FieldValueKind::Float
            if dict_type.is_none() && !has_enum_options =>
        {
            FormWidgetKind::InputNumber
        }
        FieldValueKind::Date => FormWidgetKind::Date,
        FieldValueKind::Time => FormWidgetKind::Time,
        FieldValueKind::DateTime => FormWidgetKind::DateTime,
        _ if dict_type.is_some() || has_enum_options => FormWidgetKind::Select,
        _ if is_textarea_field(field) => FormWidgetKind::Textarea,
        _ => FormWidgetKind::Input,
    };
    let form_widget = field_hint.form_widget.unwrap_or(inferred_form_widget);
    let inferred_table_display = if matches!(
        semantic_kind,
        FieldSemanticKind::Avatar | FieldSemanticKind::Image
    ) {
        TableDisplayKind::Image
    } else if matches!(
        semantic_kind,
        FieldSemanticKind::Url | FieldSemanticKind::Email
    ) {
        TableDisplayKind::Link
    } else if dict_type.is_some() {
        TableDisplayKind::DictTag
    } else if has_enum_options {
        TableDisplayKind::LocalTag
    } else if field.type_info.value_kind == FieldValueKind::Boolean {
        TableDisplayKind::BooleanTag
    } else {
        TableDisplayKind::Text
    };
    let table_display = field_hint.table_display.unwrap_or(inferred_table_display);
    let search_visible = field_hint
        .search_visible
        .unwrap_or_else(|| search_is_visible(field, &semantic_kind));
    let (search_model_key, search_param_start, search_param_end) =
        infer_search_binding(field, search_widget);
    let form_default_value =
        default_form_value(field, column, dict_type.as_ref(), has_enum_options);
    let form_required = create_enabled
        && !column.nullable
        && column.default_value.is_none()
        && !field.nullable_entity;
    let form_visibility = match (create_enabled, update_enabled) {
        _ if semantic_kind == FieldSemanticKind::Password => FormFieldVisibility::AddOnly,
        (true, true) => FormFieldVisibility::Always,
        (true, false) => FormFieldVisibility::AddOnly,
        (false, true) => FormFieldVisibility::EditOnly,
        (false, false) => FormFieldVisibility::Always,
    };

    FrontendPageFieldContext {
        name: field.name.clone(),
        camel_name: field.camel_name.clone(),
        pascal_name: field.pascal_name.clone(),
        label: normalize_ui_label(&field.label),
        semantic_kind,
        type_info: field.type_info.clone(),
        dict_type,
        local_options_literal: render_local_options_literal(&local_options),
        search_widget,
        search_visible,
        search_model_key,
        search_exclude_param: search_param_start.is_some(),
        search_param_start,
        search_param_end,
        search_value_format: search_value_format(search_widget),
        form_widget,
        form_visibility,
        table_display,
        table_width: suggested_table_width(field, semantic_kind),
        table_min_width: suggested_table_min_width(field, semantic_kind),
        table_sortable: is_sortable_field(field),
        table_overflow: should_enable_overflow(field, semantic_kind),
        form_required,
        form_default_value: form_default_value.clone(),
        form_model_type: form_model_type(
            field,
            &form_default_value,
            create_enabled,
            update_enabled,
            api,
        ),
        create_submit_value: submit_value(&field.camel_name, &form_default_value, form_required),
        update_submit_value: format!("form.{}", field.camel_name),
        search_placeholder: field_placeholder("请选择", "请输入", field, search_widget),
        form_placeholder: field_placeholder(
            "请选择",
            "请输入",
            field,
            search_widget_from_form(form_widget),
        ),
        form_upload_accept: form_upload_accept(semantic_kind),
        form_upload_button_text: form_upload_button_text(semantic_kind),
        form_rule_trigger: form_rule_trigger(form_widget).to_string(),
        bool_true_label: boolean_true_label(field),
        bool_false_label: boolean_false_label(field),
    }
}

fn select_field_contexts(
    label: &str,
    field_map: &BTreeMap<String, FrontendPageFieldContext>,
    configured: Option<&[String]>,
    allowed_field_names: &BTreeSet<String>,
    default_field_names: Vec<String>,
) -> Result<Vec<FrontendPageFieldContext>, McpError> {
    let field_names = configured
        .map(|fields| fields.to_vec())
        .unwrap_or(default_field_names);
    let mut result = Vec::with_capacity(field_names.len());
    for field_name in field_names {
        if !allowed_field_names.contains(&field_name) {
            let available = allowed_field_names
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(McpError::invalid_params(
                format!(
                    "field `{field_name}` is not valid for {label}; allowed fields: {available}"
                ),
                None,
            ));
        }
        let Some(field) = field_map.get(&field_name) else {
            let available = field_map.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(McpError::invalid_params(
                format!("unknown {label} field `{field_name}`; available fields: {available}"),
                None,
            ));
        };
        result.push(field.clone());
    }
    Ok(result)
}

fn default_search_field_names(
    crud_context: &CrudGenerationContext,
    field_map: &BTreeMap<String, FrontendPageFieldContext>,
) -> Vec<String> {
    let mut candidates = crud_context
        .query_fields
        .iter()
        .filter(|field| field.name != crud_context.primary_key.name)
        .filter(|field| !should_skip_default_search_field(field))
        .filter_map(|field| field_map.get(&field.name))
        .filter(|field| field.search_visible)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        search_priority(right)
            .cmp(&search_priority(left))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut selected = Vec::new();
    let mut seen_model_keys = BTreeSet::new();
    for field in &candidates {
        if seen_model_keys.insert(field.search_model_key.clone()) {
            selected.push(field.name.clone());
        }
    }

    if !selected.iter().any(|field_name| {
        field_map
            .get(field_name)
            .is_some_and(|field| field.search_exclude_param)
    }) {
        if let Some(range_field) = candidates
            .into_iter()
            .find(|field| field.search_exclude_param && !selected.contains(&field.name))
        {
            if seen_model_keys.insert(range_field.search_model_key.clone()) {
                selected.push(range_field.name.clone());
            }
        }
    }

    selected
}

fn ensure_unique_search_model_keys(fields: &[FrontendPageFieldContext]) -> Result<(), McpError> {
    let mut model_key_to_field = BTreeMap::<String, String>::new();
    for field in fields {
        if let Some(previous) =
            model_key_to_field.insert(field.search_model_key.clone(), field.name.clone())
        {
            return Err(McpError::invalid_params(
                format!(
                    "search fields `{previous}` and `{}` both map to search model key `{}`; choose only one or override field_hints.search_widget",
                    field.name, field.search_model_key
                ),
                None,
            ));
        }
    }
    Ok(())
}

fn default_table_field_names(crud_context: &CrudGenerationContext) -> Vec<String> {
    crud_context
        .read_fields
        .iter()
        .filter(|field| !matches!(field.type_info.value_kind, FieldValueKind::Json))
        .filter(|field| !should_skip_default_table_field(field))
        .map(|field| field.name.clone())
        .collect()
}

fn default_form_field_names(crud_context: &CrudGenerationContext) -> Vec<String> {
    let mut seen = BTreeSet::new();
    crud_context
        .create_fields
        .iter()
        .chain(crud_context.update_fields.iter())
        .filter(|field| !matches!(field.type_info.value_kind, FieldValueKind::Json))
        .filter(|field| !should_skip_default_form_field(field))
        .filter_map(|field| seen.insert(field.name.clone()).then(|| field.name.clone()))
        .collect()
}

fn collect_required_dict_types<'a>(
    field_groups: impl IntoIterator<Item = &'a Vec<FrontendPageFieldContext>>,
) -> Vec<String> {
    field_groups
        .into_iter()
        .flat_map(|fields| fields.iter())
        .filter_map(|field| field.dict_type.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn build_page_naming_context(
    crud_context: &CrudGenerationContext,
    table_fields: &[FrontendPageFieldContext],
) -> FrontendPageNamingContext {
    let resource_pascal = &crud_context.names.resource_pascal;
    let folder_name = crud_context.table.file_stem.clone();
    let item_prop_name = format!("{}Data", lower_first(resource_pascal));
    FrontendPageNamingContext {
        folder_name: folder_name.clone(),
        component_name: resource_pascal.clone(),
        search_component_name: format!("{resource_pascal}Search"),
        dialog_component_name: format!("{resource_pascal}Dialog"),
        search_file_name: format!("{folder_name}-search.vue"),
        dialog_file_name: format!("{folder_name}-dialog.vue"),
        list_item_alias: format!("{resource_pascal}ListItem"),
        current_item_ref_name: format!("current{resource_pascal}Data"),
        item_prop_name: item_prop_name.clone(),
        item_prop_attr_name: camel_to_kebab(&item_prop_name),
        delete_display_field: select_delete_display_field(crud_context, table_fields),
    }
}

fn select_delete_display_field(
    crud_context: &CrudGenerationContext,
    table_fields: &[FrontendPageFieldContext],
) -> String {
    let preferred_name = table_fields
        .iter()
        .find(|field| {
            field.camel_name != crud_context.primary_key.camel_name
                && matches!(field.type_info.value_kind, FieldValueKind::String)
                && matches!(
                    field.name.as_str(),
                    "name"
                        | "title"
                        | "code"
                        | "role_name"
                        | "user_name"
                        | "dict_name"
                        | "dict_type"
                )
        })
        .or_else(|| {
            table_fields.iter().find(|field| {
                field.camel_name != crud_context.primary_key.camel_name
                    && matches!(field.type_info.value_kind, FieldValueKind::String)
            })
        })
        .or_else(|| {
            table_fields
                .iter()
                .find(|field| field.camel_name != crud_context.primary_key.camel_name)
        })
        .map(|field| field.camel_name.clone())
        .unwrap_or_else(|| crud_context.primary_key.camel_name.clone());
    preferred_name
}

fn infer_field_semantic(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
) -> FieldSemanticKind {
    let mut hints = vec![field.name.as_str(), field.label.as_str()];
    if let Some(comment) = column.comment.as_deref() {
        hints.push(comment);
    }
    let hint = hints.join(" ").to_ascii_lowercase();

    if contains_any(
        &hint,
        &["password", "passwd", "pwd", "secret", "token", "密码"],
    ) {
        return FieldSemanticKind::Password;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(&hint, &["avatar", "头像"])
    {
        return FieldSemanticKind::Avatar;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(
            &hint,
            &[
                "image",
                "img",
                "picture",
                "photo",
                "cover",
                "logo",
                "banner",
                "thumb",
                "图片",
                "照片",
                "封面",
                "缩略图",
            ],
        )
    {
        return FieldSemanticKind::Image;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(
            &hint,
            &["file", "attachment", "document", "附件", "文件", "文档"],
        )
    {
        return FieldSemanticKind::File;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(&hint, &["icon", "图标"])
    {
        return FieldSemanticKind::Icon;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(&hint, &["email", "邮箱", "mail"])
    {
        return FieldSemanticKind::Email;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(&hint, &["phone", "mobile", "tel", "手机", "电话"])
    {
        return FieldSemanticKind::Phone;
    }
    if field.type_info.value_kind == FieldValueKind::String
        && contains_any(&hint, &["url", "link", "链接", "地址"])
    {
        return FieldSemanticKind::Url;
    }
    if is_textarea_field(field) {
        return FieldSemanticKind::RichText;
    }
    FieldSemanticKind::Plain
}

fn search_is_visible(field: &FieldGenerationContext, semantic_kind: &FieldSemanticKind) -> bool {
    if matches!(
        semantic_kind,
        FieldSemanticKind::Password
            | FieldSemanticKind::Avatar
            | FieldSemanticKind::Image
            | FieldSemanticKind::File
            | FieldSemanticKind::Icon
            | FieldSemanticKind::RichText
    ) {
        return false;
    }

    !matches!(
        field.type_info.value_kind,
        FieldValueKind::Json | FieldValueKind::Other | FieldValueKind::Uuid
    )
}

fn search_should_use_range(
    field: &FieldGenerationContext,
    semantic_kind: &FieldSemanticKind,
) -> bool {
    if matches!(
        semantic_kind,
        FieldSemanticKind::Avatar
            | FieldSemanticKind::Image
            | FieldSemanticKind::File
            | FieldSemanticKind::Password
    ) {
        return false;
    }

    matches!(
        field.type_info.value_kind,
        FieldValueKind::Date | FieldValueKind::DateTime
    )
}

fn infer_search_binding(
    field: &FieldGenerationContext,
    search_widget: SearchWidgetKind,
) -> (String, Option<String>, Option<String>) {
    match search_widget {
        SearchWidgetKind::DateRange | SearchWidgetKind::DateTimeRange => (
            format!("{}Range", field.camel_name),
            Some(format!("{}Start", field.camel_name)),
            Some(format!("{}End", field.camel_name)),
        ),
        _ => (field.camel_name.clone(), None, None),
    }
}

fn search_value_format(search_widget: SearchWidgetKind) -> Option<String> {
    match search_widget {
        SearchWidgetKind::Date | SearchWidgetKind::DateRange => Some("YYYY-MM-DD".to_string()),
        SearchWidgetKind::DateTime | SearchWidgetKind::DateTimeRange => {
            Some("YYYY-MM-DD HH:mm:ss".to_string())
        }
        SearchWidgetKind::Time => Some("HH:mm:ss".to_string()),
        SearchWidgetKind::Input
        | SearchWidgetKind::Number
        | SearchWidgetKind::Select
        | SearchWidgetKind::RadioGroup => None,
    }
}

fn search_priority(field: &FrontendPageFieldContext) -> i32 {
    let mut score = 0;
    if field.dict_type.is_some()
        || field.type_info.value_kind == FieldValueKind::Boolean
        || field.local_options_literal.is_some()
    {
        score += 90;
    }
    if matches!(
        field.search_widget,
        SearchWidgetKind::DateRange | SearchWidgetKind::DateTimeRange
    ) {
        score += 75;
    }
    if contains_any(
        &field.name,
        &[
            "name", "title", "code", "status", "enabled", "type", "phone", "email", "gender",
        ],
    ) {
        score += 70;
    }
    if is_textarea_field_like_name(&field.name) {
        score -= 40;
    }
    score
}

fn suggested_table_width(
    field: &FieldGenerationContext,
    semantic_kind: FieldSemanticKind,
) -> Option<u16> {
    match field.type_info.value_kind {
        _ if matches!(
            semantic_kind,
            FieldSemanticKind::Avatar | FieldSemanticKind::Image
        ) =>
        {
            Some(96)
        }
        _ if semantic_kind == FieldSemanticKind::Url => Some(220),
        FieldValueKind::Boolean => Some(110),
        FieldValueKind::DateTime => Some(180),
        FieldValueKind::Date => Some(140),
        FieldValueKind::Time => Some(120),
        FieldValueKind::Integer | FieldValueKind::Float | FieldValueKind::Decimal => Some(120),
        _ if field.name == "id" => Some(100),
        _ => None,
    }
}

fn suggested_table_min_width(
    field: &FieldGenerationContext,
    semantic_kind: FieldSemanticKind,
) -> Option<u16> {
    match field.type_info.value_kind {
        _ if semantic_kind == FieldSemanticKind::Url => Some(220),
        FieldValueKind::String => Some(if is_textarea_field(field) { 180 } else { 140 }),
        FieldValueKind::Uuid => Some(180),
        FieldValueKind::Other => Some(160),
        _ => None,
    }
}

fn is_sortable_field(field: &FieldGenerationContext) -> bool {
    matches!(
        field.type_info.value_kind,
        FieldValueKind::Boolean
            | FieldValueKind::Integer
            | FieldValueKind::Float
            | FieldValueKind::Decimal
            | FieldValueKind::Date
            | FieldValueKind::Time
            | FieldValueKind::DateTime
    )
}

fn should_enable_overflow(
    field: &FieldGenerationContext,
    semantic_kind: FieldSemanticKind,
) -> bool {
    !matches!(
        semantic_kind,
        FieldSemanticKind::Avatar
            | FieldSemanticKind::Image
            | FieldSemanticKind::Url
            | FieldSemanticKind::Icon
    ) && (is_textarea_field(field)
        || matches!(
            field.type_info.value_kind,
            FieldValueKind::String
                | FieldValueKind::Uuid
                | FieldValueKind::Json
                | FieldValueKind::Other
        ))
}

fn default_form_value(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
    dict_type: Option<&String>,
    has_enum_options: bool,
) -> String {
    if dict_type.is_some() || has_enum_options {
        return "undefined".to_string();
    }

    match field.type_info.value_kind {
        FieldValueKind::String | FieldValueKind::Uuid | FieldValueKind::Decimal => "''".to_string(),
        FieldValueKind::Boolean => {
            if column
                .default_value
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        FieldValueKind::Integer
        | FieldValueKind::Float
        | FieldValueKind::Date
        | FieldValueKind::Time
        | FieldValueKind::DateTime
        | FieldValueKind::Enum
        | FieldValueKind::Json
        | FieldValueKind::Other => "undefined".to_string(),
    }
}

fn form_upload_accept(semantic_kind: FieldSemanticKind) -> Option<String> {
    match semantic_kind {
        FieldSemanticKind::Avatar | FieldSemanticKind::Image => Some("image/*".to_string()),
        FieldSemanticKind::File => None,
        _ => None,
    }
}

fn form_upload_button_text(semantic_kind: FieldSemanticKind) -> Option<String> {
    match semantic_kind {
        FieldSemanticKind::Avatar => Some("上传头像".to_string()),
        FieldSemanticKind::Image => Some("上传图片".to_string()),
        FieldSemanticKind::File => Some("上传文件".to_string()),
        _ => None,
    }
}

fn form_rule_trigger(form_widget: FormWidgetKind) -> &'static str {
    match form_widget {
        FormWidgetKind::Input | FormWidgetKind::Password | FormWidgetKind::Textarea => "blur",
        FormWidgetKind::Switch
        | FormWidgetKind::Select
        | FormWidgetKind::InputNumber
        | FormWidgetKind::Date
        | FormWidgetKind::Time
        | FormWidgetKind::DateTime
        | FormWidgetKind::ImageUpload
        | FormWidgetKind::FileUpload => "change",
    }
}

fn local_options(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
) -> Vec<EnumOptionContext> {
    if !field.type_info.enum_options.is_empty() {
        return field.type_info.enum_options.clone();
    }

    column
        .enum_values
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|value| EnumOptionContext {
            label: value.clone(),
            value_literal: format!("{value:?}"),
            value_kind: crate::tools::generation_context::EnumOptionValueKind::String,
        })
        .collect()
}

fn render_local_options_literal(local_options: &[EnumOptionContext]) -> Option<String> {
    if local_options.is_empty() {
        return None;
    }

    Some(format!(
        "[{}]",
        local_options
            .iter()
            .map(|option| format!(
                "{{ label: {:?}, value: {} }}",
                option.label, option.value_literal
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn form_model_type(
    field: &FieldGenerationContext,
    default_value: &str,
    create_enabled: bool,
    update_enabled: bool,
    api: &FrontendPageApiContext,
) -> String {
    let base = if create_enabled {
        format!(
            "Api.{}.{}['{}']",
            api.namespace, api.create_params_type_name, field.camel_name
        )
    } else if update_enabled {
        format!(
            "Api.{}.{}['{}']",
            api.namespace, api.update_params_type_name, field.camel_name
        )
    } else {
        field.type_info.ts_input.clone()
    };
    if default_value == "undefined" && !base.contains("undefined") {
        format!("{base} | undefined")
    } else {
        base
    }
}

fn submit_value(field_camel_name: &str, form_default_value: &str, form_required: bool) -> String {
    if form_required && form_default_value == "undefined" {
        format!("form.{field_camel_name}!")
    } else {
        format!("form.{field_camel_name}")
    }
}

fn search_widget_from_form(widget: FormWidgetKind) -> SearchWidgetKind {
    match widget {
        FormWidgetKind::Select | FormWidgetKind::Switch => SearchWidgetKind::Select,
        FormWidgetKind::InputNumber => SearchWidgetKind::Number,
        FormWidgetKind::Date => SearchWidgetKind::Date,
        FormWidgetKind::Time => SearchWidgetKind::Time,
        FormWidgetKind::DateTime => SearchWidgetKind::DateTime,
        FormWidgetKind::Input
        | FormWidgetKind::Password
        | FormWidgetKind::Textarea
        | FormWidgetKind::ImageUpload
        | FormWidgetKind::FileUpload => SearchWidgetKind::Input,
    }
}

fn field_placeholder(
    select_prefix: &str,
    input_prefix: &str,
    field: &FieldGenerationContext,
    widget: SearchWidgetKind,
) -> String {
    match widget {
        SearchWidgetKind::RadioGroup => String::new(),
        SearchWidgetKind::Select
        | SearchWidgetKind::Date
        | SearchWidgetKind::DateRange
        | SearchWidgetKind::Time
        | SearchWidgetKind::DateTime
        | SearchWidgetKind::DateTimeRange => {
            format!("{select_prefix}{}", normalize_ui_label(&field.label))
        }
        SearchWidgetKind::Number | SearchWidgetKind::Input => {
            format!("{input_prefix}{}", normalize_ui_label(&field.label))
        }
    }
}

fn contains_any(hint: &str, needles: &[&str]) -> bool {
    let hint = hint.to_ascii_lowercase();
    needles
        .iter()
        .any(|needle| hint.contains(&needle.to_ascii_lowercase()))
}

fn is_textarea_field_like_name(name: &str) -> bool {
    contains_any(name, &["description", "remark", "content", "note", "memo"])
}

fn is_textarea_field(field: &FieldGenerationContext) -> bool {
    let label = field.label.as_str();
    is_textarea_field_like_name(field.name.as_str())
        || label.contains("描述")
        || label.contains("备注")
        || label.contains("内容")
        || label.contains("说明")
}

fn is_gender_like_field(field: &FieldGenerationContext) -> bool {
    contains_any(
        &format!("{} {}", field.name, field.label),
        &["gender", "sex", "性别"],
    )
}

fn should_skip_default_form_field(field: &FieldGenerationContext) -> bool {
    contains_any(
        field.name.as_str(),
        &[
            "create_by",
            "update_by",
            "create_time",
            "update_time",
            "created_at",
            "updated_at",
            "deleted",
            "deleted_at",
            "delete_time",
            "sort",
            "version",
        ],
    )
}

fn should_skip_default_search_field(field: &FieldGenerationContext) -> bool {
    contains_any(
        field.name.as_str(),
        &[
            "create_by",
            "update_by",
            "deleted",
            "deleted_at",
            "delete_time",
            "password",
        ],
    )
}

fn should_skip_default_table_field(field: &FieldGenerationContext) -> bool {
    contains_any(
        field.name.as_str(),
        &[
            "create_by",
            "update_by",
            "deleted",
            "deleted_at",
            "delete_time",
        ],
    )
}

fn normalize_ui_label(label: &str) -> String {
    let trimmed = label.trim();
    for separator in ['（', '('] {
        if let Some((prefix, _)) = trimmed.split_once(separator) {
            let prefix = prefix.trim();
            if !prefix.is_empty() {
                return prefix.to_string();
            }
        }
    }
    for separator in ['：', ':'] {
        if let Some((prefix, suffix)) = trimmed.split_once(separator) {
            let suffix = suffix.trim();
            if suffix.chars().any(|ch| ch.is_ascii_digit())
                || suffix.contains('-')
                || suffix.contains('男')
                || suffix.contains('女')
                || suffix.contains('启')
                || suffix.contains('停')
            {
                let prefix = prefix.trim();
                if !prefix.is_empty() {
                    return prefix.to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

fn boolean_true_label(field: &FieldGenerationContext) -> String {
    if field.name.contains("enabled") {
        "启用".to_string()
    } else {
        "是".to_string()
    }
}

fn boolean_false_label(field: &FieldGenerationContext) -> String {
    if field.name.contains("enabled") {
        "禁用".to_string()
    } else {
        "否".to_string()
    }
}

fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            let mut result = String::new();
            result.push(first.to_ascii_lowercase());
            result.push_str(chars.as_str());
            result
        }
        None => String::new(),
    }
}

fn camel_to_kebab(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .fold(String::new(), |mut output, (idx, ch)| {
            if ch.is_ascii_uppercase() {
                if idx > 0 {
                    output.push('-');
                }
                output.push(ch.to_ascii_lowercase());
            } else {
                output.push(ch);
            }
            output
        })
}

#[cfg(test)]
mod tests {
    use crate::table_tools::schema::{TableCheckConstraintSchema, TableIndexSchema};

    use super::*;

    fn sample_user_schema() -> TableSchema {
        TableSchema {
            schema: "public".to_string(),
            table: "sys_user".to_string(),
            comment: Some("用户管理".to_string()),
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
                    comment: Some("头像地址".to_string()),
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
                    comment: Some("用户状态".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "gender".to_string(),
                    pg_type: "smallint".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("性别".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "phone".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("手机号".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "email".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("邮箱".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "description".to_string(),
                    pg_type: "text".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("备注".to_string()),
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
    async fn generator_writes_semantic_frontend_page_files() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();

        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_user.rs"),
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

/// 性别
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum Gender {
    /// 男
    #[sea_orm(num_value = 1)]
    Male,
    /// 女
    #[sea_orm(num_value = 2)]
    Female,
}

#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub user_name: String,
    pub password: String,
    pub avatar: String,
    pub status: UserStatus,
    pub gender: Gender,
    pub phone: String,
    pub email: String,
    pub description: Option<String>,
    pub create_time: DateTime,
}
"#,
        )
        .unwrap();

        let generator = FrontendPageGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendPageRequest {
                schema: sample_user_schema(),
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/views/system".to_string()),
                target_preset: FrontendTargetPreset::SummerMcp,
                api_import_path: None,
                api_namespace: None,
                api_list_item_type_name: None,
                api_detail_type_name: None,
                dict_bindings: BTreeMap::from([("status".to_string(), "user_status".to_string())]),
                field_hints: BTreeMap::from([(
                    "email".to_string(),
                    FrontendFieldUiHint {
                        table_display: Some(TableDisplayKind::Link),
                        ..Default::default()
                    },
                )]),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        let index_file = std::fs::read_to_string(&result.index_file).unwrap();
        assert!(index_file.contains("UserSearch"));
        assert!(index_file.contains("UserDialog"));
        assert!(index_file.contains("fetchGetUserList"));
        assert!(index_file.contains("from '@/api/user'"));
        assert!(index_file.contains("getDictLabel('user_status'"));
        assert!(index_file.contains("ElImage"));
        assert!(index_file.contains("mailto:"));

        let search_file = std::fs::read_to_string(&result.search_file).unwrap();
        assert!(search_file.contains("getDict('user_status')"));
        assert!(search_file.contains("type: 'datetimerange'"));
        assert!(search_file.contains("type SearchFormModel = {"));
        assert!(search_file.contains("key: 'createTimeRange'"));
        assert!(search_file.contains("Api.User.UserSearchParams['createTimeStart']"));
        assert!(search_file.contains("Api.User.UserSearchParams['createTimeEnd']"));

        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();
        assert!(dialog_file.contains("fetchCreateUser"));
        assert!(dialog_file.contains("fetchGetUserDetail"));
        assert!(dialog_file.contains("fetchUpdateUser"));
        assert!(dialog_file.contains("type UserListItem = Api.User.UserVo"));
        assert!(dialog_file.contains("type UserListItemDetail = Api.User.UserDetailVo"));
        assert!(dialog_file.contains("type=\"textarea\""));
        assert!(dialog_file.contains("ArtFileUpload"));
        assert!(dialog_file.contains("handleAvatarUploadSuccess"));
        assert!(
            dialog_file
                .contains("const avatarUploadFiles = ref<Api.FileUpload.FileUploadVo[]>([])")
        );
        assert!(dialog_file.contains(r#"label: "男", value: 1"#));
        assert!(dialog_file.contains("trigger: 'change'"));
        assert!(
            dialog_file.contains("const detail: UserListItemDetail = await fetchGetUserDetail(id)")
        );

        assert_eq!(result.required_dict_types, vec!["user_status".to_string()]);
        assert_eq!(result.api_import_path, "@/api/user");
        assert_eq!(result.api_namespace, "User");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_supports_advanced_api_contract_overrides() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-advanced-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();

        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_user.rs"),
            r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub user_name: String,
    pub password: String,
    pub status: i16,
}
"#,
        )
        .unwrap();

        let generator = FrontendPageGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendPageRequest {
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
                output_dir: Some("generated/views/system".to_string()),
                target_preset: FrontendTargetPreset::SummerMcp,
                api_import_path: Some("@/api/system-manage".to_string()),
                api_namespace: Some("SystemManage".to_string()),
                api_list_item_type_name: Some("UserListItem".to_string()),
                api_detail_type_name: Some("UserDetailVo".to_string()),
                dict_bindings: BTreeMap::new(),
                field_hints: BTreeMap::new(),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();
        assert!(dialog_file.contains("from '@/api/system-manage'"));
        assert!(dialog_file.contains("type UserListItem = Api.SystemManage.UserListItem"));
        assert!(dialog_file.contains("type UserListItemDetail = Api.SystemManage.UserDetailVo"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_supports_art_design_pro_page_layout() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-adp-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_user.rs"),
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

        let generator = FrontendPageGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendPageRequest {
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
                api_import_path: None,
                api_namespace: None,
                api_list_item_type_name: None,
                api_detail_type_name: None,
                dict_bindings: BTreeMap::new(),
                field_hints: BTreeMap::new(),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        assert_eq!(
            result.page_dir,
            root.join("generated/art-design-pro/src/views/system/user")
        );
        assert_eq!(
            result.index_file,
            root.join("generated/art-design-pro/src/views/system/user/index.vue")
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
