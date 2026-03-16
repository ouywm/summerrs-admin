use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
const FRONTEND_PAGE_MACROS_TEMPLATE_NAME: &str = "frontend/page/_macros.j2";
const FORM_DRAWER_FIELD_THRESHOLD: usize = 8;
const FORM_DIALOG_WIDTH: &str = "36%";
const FORM_DRAWER_SIZE: &str = "760px";
const PASSWORD_KEYWORDS: &[&str] = &["password", "passwd", "pwd", "secret", "token", "密码"];
const AVATAR_KEYWORDS: &[&str] = &["avatar", "头像"];
const IMAGE_KEYWORDS: &[&str] = &[
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
];
const FILE_KEYWORDS: &[&str] = &["file", "attachment", "document", "附件", "文件", "文档"];
const ICON_KEYWORDS: &[&str] = &["icon", "图标"];
const EMAIL_KEYWORDS: &[&str] = &["email", "邮箱", "mail"];
const PHONE_KEYWORDS: &[&str] = &["phone", "mobile", "tel", "手机", "电话"];
const URL_KEYWORDS: &[&str] = &["url", "link", "链接", "地址"];
const GENDER_KEYWORDS: &[&str] = &["gender", "sex", "性别"];
const TEXTAREA_FIELD_NAME_KEYWORDS: &[&str] = &["description", "remark", "content", "note", "memo"];
const SEARCH_PRIORITY_KEYWORDS: &[&str] = &[
    "name", "title", "code", "status", "enabled", "type", "phone", "email", "gender",
];
const SKIP_DEFAULT_FORM_FIELD_KEYWORDS: &[&str] = &[
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
];
const SKIP_DEFAULT_SEARCH_FIELD_KEYWORDS: &[&str] = &[
    "create_by",
    "update_by",
    "deleted",
    "deleted_at",
    "delete_time",
    "password",
];
const SKIP_DEFAULT_TABLE_FIELD_KEYWORDS: &[&str] = &[
    "create_by",
    "update_by",
    "deleted",
    "deleted_at",
    "delete_time",
];

const FRONTEND_PAGE_TEMPLATES: [EmbeddedTemplate; 4] = [
    EmbeddedTemplate {
        name: FRONTEND_PAGE_MACROS_TEMPLATE_NAME,
        source: include_str!("../../templates/frontend/page/_macros.j2"),
    },
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
    pub field_ui_meta: BTreeMap<String, FrontendFieldUiMeta>,
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
    form_container: FrontendFormContainerContext,
    api: FrontendPageApiContext,
    flags: FrontendPageFlags,
    search_fields: Vec<FrontendPageFieldContext>,
    table_fields: Vec<FrontendPageFieldContext>,
    form_fields: Vec<FrontendPageFieldContext>,
    required_dict_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FrontendFormContainerContext {
    use_drawer: bool,
    size: String,
}

impl FrontendFormContainerContext {
    fn new(form_fields: &[FrontendPageFieldContext]) -> Self {
        let use_drawer = form_fields.len() > FORM_DRAWER_FIELD_THRESHOLD;
        Self {
            use_drawer,
            size: if use_drawer {
                FORM_DRAWER_SIZE.to_string()
            } else {
                FORM_DIALOG_WIDTH.to_string()
            },
        }
    }
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
    uses_form_editor: bool,
    uses_avatar_placeholder: bool,
    uses_range_search: bool,
    has_search_fields: bool,
}

#[derive(Debug, Default)]
struct SelectedFieldSummary {
    required_dict_types: BTreeSet<String>,
    uses_table_dict: bool,
    uses_search_dict: bool,
    uses_form_dict: bool,
    uses_table_tag: bool,
    uses_table_image: bool,
    uses_form_upload: bool,
    uses_form_editor: bool,
    uses_avatar_placeholder: bool,
    uses_range_search: bool,
    has_search_fields: bool,
}

#[derive(Debug, Clone)]
struct ResolvedFieldOptionContext {
    dict_type: Option<String>,
    local_options_literal: Option<String>,
    option_items_literal: String,
    has_enum_options: bool,
    bool_true_label: String,
    bool_false_label: String,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedFieldUiComponents {
    semantic_kind: FieldSemanticKind,
    search_widget: SearchWidgetKind,
    search_visible: bool,
    form_widget: FormWidgetKind,
    form_visibility: FormFieldVisibility,
    table_display: TableDisplayKind,
}

#[derive(Debug, Clone)]
struct ResolvedSearchBinding {
    model_key: String,
    param_start: Option<String>,
    param_end: Option<String>,
}

#[derive(Debug, Clone)]
struct FieldSemanticSignals {
    hint_text: String,
    string_value: bool,
    textarea_like: bool,
    gender_like: bool,
}

impl FieldSemanticSignals {
    fn from_field(field: &FieldGenerationContext, column: &TableColumnSchema) -> Self {
        let mut hints = vec![field.name.as_str(), field.label.as_str()];
        if let Some(comment) = column.comment.as_deref() {
            hints.push(comment);
        }

        Self {
            hint_text: hints.join(" ").to_ascii_lowercase(),
            string_value: field.type_info.value_kind == FieldValueKind::String,
            textarea_like: is_textarea_field_like_name(field.name.as_str())
                || field.label.contains("描述")
                || field.label.contains("备注")
                || field.label.contains("内容")
                || field.label.contains("说明"),
            gender_like: contains_any(&format!("{} {}", field.name, field.label), GENDER_KEYWORDS),
        }
    }

    fn contains_any(&self, needles: &[&str]) -> bool {
        needles
            .iter()
            .any(|needle| self.hint_text.contains(&needle.to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, Copy)]
struct FormUploadConfig {
    accept: Option<&'static str>,
    button_text: &'static str,
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
    option_items_literal: String,
    search_widget: SearchWidgetKind,
    search_visible: bool,
    search_model_key: String,
    search_exclude_param: bool,
    search_param_start: Option<String>,
    search_param_end: Option<String>,
    search_value_format: Option<String>,
    form_widget: FormWidgetKind,
    form_visibility: FormFieldVisibility,
    form_span: Option<u8>,
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
    search_props_literal: Option<String>,
    form_placeholder: String,
    form_props_literal: Option<String>,
    form_upload_accept: Option<String>,
    form_upload_button_text: Option<String>,
    form_upload_props_literal: Option<String>,
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

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendFieldUiMeta {
    pub dict_type: Option<String>,
    pub semantic: Option<FieldSemanticKind>,
    pub search: Option<SearchFieldUiMeta>,
    pub form: Option<FormFieldUiMeta>,
    pub table: Option<TableFieldUiMeta>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchFieldUiMeta {
    pub component: Option<SearchWidgetKind>,
    pub placeholder: Option<String>,
    pub props: Option<JsonValue>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormFieldUiMeta {
    pub component: Option<FormWidgetKind>,
    pub required: Option<bool>,
    pub placeholder: Option<String>,
    pub props: Option<JsonValue>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableFieldUiMeta {
    pub component: Option<TableDisplayKind>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedFrontendFieldUiMeta {
    dict_type: Option<String>,
    semantic: Option<FieldSemanticKind>,
    search_visible: Option<bool>,
    search_widget: Option<SearchWidgetKind>,
    search_placeholder: Option<String>,
    search_props: Option<JsonValue>,
    form_widget: Option<FormWidgetKind>,
    form_required: Option<bool>,
    form_placeholder: Option<String>,
    form_props: Option<JsonValue>,
    table_display: Option<TableDisplayKind>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchWidgetKind {
    Input,
    Number,
    Select,
    RadioGroup,
    CheckboxGroup,
    Cascader,
    TreeSelect,
    Date,
    DateRange,
    Time,
    DateTime,
    DateTimeRange,
}

impl SearchWidgetKind {
    fn value_format(self) -> Option<&'static str> {
        match self {
            Self::Date | Self::DateRange => Some("YYYY-MM-DD"),
            Self::DateTime | Self::DateTimeRange => Some("YYYY-MM-DDTHH:mm:ss"),
            Self::Time => Some("HH:mm:ss"),
            Self::Input
            | Self::Number
            | Self::Select
            | Self::RadioGroup
            | Self::CheckboxGroup
            | Self::Cascader
            | Self::TreeSelect => None,
        }
    }

    fn uses_select_placeholder(self) -> bool {
        matches!(
            self,
            Self::Select
                | Self::Cascader
                | Self::TreeSelect
                | Self::Date
                | Self::DateRange
                | Self::Time
                | Self::DateTime
                | Self::DateTimeRange
        )
    }

    fn hides_placeholder(self) -> bool {
        matches!(self, Self::RadioGroup | Self::CheckboxGroup)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FormWidgetKind {
    Input,
    Password,
    Textarea,
    Switch,
    Select,
    RadioGroup,
    CheckboxGroup,
    Cascader,
    TreeSelect,
    InputNumber,
    Date,
    Time,
    DateTime,
    Editor,
    ImageUpload,
    FileUpload,
}

impl FormWidgetKind {
    fn rule_trigger(self) -> &'static str {
        match self {
            Self::Input | Self::Password | Self::Textarea => "blur",
            Self::Switch
            | Self::Select
            | Self::RadioGroup
            | Self::CheckboxGroup
            | Self::Cascader
            | Self::TreeSelect
            | Self::InputNumber
            | Self::Date
            | Self::Time
            | Self::DateTime
            | Self::Editor
            | Self::ImageUpload
            | Self::FileUpload => "change",
        }
    }

    fn default_search_widget(self) -> SearchWidgetKind {
        match self {
            Self::Select | Self::Switch | Self::RadioGroup | Self::CheckboxGroup => {
                SearchWidgetKind::Select
            }
            Self::Cascader => SearchWidgetKind::Cascader,
            Self::TreeSelect => SearchWidgetKind::TreeSelect,
            Self::InputNumber => SearchWidgetKind::Number,
            Self::Date => SearchWidgetKind::Date,
            Self::Time => SearchWidgetKind::Time,
            Self::DateTime => SearchWidgetKind::DateTime,
            Self::Input
            | Self::Password
            | Self::Textarea
            | Self::Editor
            | Self::ImageUpload
            | Self::FileUpload => SearchWidgetKind::Input,
        }
    }

    fn default_span(self, use_drawer: bool) -> u8 {
        if !use_drawer {
            return 24;
        }

        match self {
            Self::Textarea | Self::Editor | Self::ImageUpload | Self::FileUpload => 24,
            _ => 12,
        }
    }
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

impl FieldSemanticKind {
    fn is_avatar(self) -> bool {
        self == Self::Avatar
    }

    fn is_image_like(self) -> bool {
        matches!(self, Self::Avatar | Self::Image)
    }

    fn is_link_like(self) -> bool {
        matches!(self, Self::Url | Self::Email)
    }

    fn hides_search(self) -> bool {
        matches!(
            self,
            Self::Password | Self::Avatar | Self::Image | Self::File | Self::Icon | Self::RichText
        )
    }

    fn blocks_range_search(self) -> bool {
        matches!(
            self,
            Self::Avatar | Self::Image | Self::File | Self::Password
        )
    }

    fn default_table_display(self) -> Option<TableDisplayKind> {
        if self.is_image_like() {
            Some(TableDisplayKind::Image)
        } else if self.is_link_like() {
            Some(TableDisplayKind::Link)
        } else {
            None
        }
    }

    fn upload_config(self) -> Option<FormUploadConfig> {
        match self {
            Self::Avatar => Some(FormUploadConfig {
                accept: Some("image/*"),
                button_text: "上传头像",
            }),
            Self::Image => Some(FormUploadConfig {
                accept: Some("image/*"),
                button_text: "上传图片",
            }),
            Self::File => Some(FormUploadConfig {
                accept: None,
                button_text: "上传文件",
            }),
            _ => None,
        }
    }
}

impl ResolvedFrontendFieldUiMeta {
    fn from_hint(hint: FrontendFieldUiHint) -> Self {
        Self {
            semantic: hint.semantic,
            search_visible: hint.search_visible,
            search_widget: hint.search_widget,
            form_widget: hint.form_widget,
            table_display: hint.table_display,
            ..Self::default()
        }
    }

    fn apply_dict_type_override(&mut self, dict_type: Option<String>) {
        self.dict_type = dict_type.filter(|value| !value.trim().is_empty());
    }

    fn apply_meta(&mut self, meta: FrontendFieldUiMeta) {
        if let Some(dict_type) = normalize_optional_text(meta.dict_type) {
            self.dict_type = Some(dict_type);
        }
        if let Some(semantic) = meta.semantic {
            self.semantic = Some(semantic);
        }
        if let Some(search) = meta.search {
            if let Some(component) = search.component {
                self.search_widget = Some(component);
            }
            if let Some(placeholder) = normalize_optional_text(search.placeholder) {
                self.search_placeholder = Some(placeholder);
            }
            if let Some(props) = normalize_optional_object(search.props) {
                self.search_props = Some(props);
            }
        }
        if let Some(form) = meta.form {
            if let Some(component) = form.component {
                self.form_widget = Some(component);
            }
            if let Some(required) = form.required {
                self.form_required = Some(required);
            }
            if let Some(placeholder) = normalize_optional_text(form.placeholder) {
                self.form_placeholder = Some(placeholder);
            }
            if let Some(props) = normalize_optional_object(form.props) {
                self.form_props = Some(props);
            }
        }
        if let Some(table) = meta.table {
            if let Some(component) = table.component {
                self.table_display = Some(component);
            }
        }
    }
}

fn resolve_frontend_field_ui_meta(
    dict_type: Option<String>,
    field_hint: Option<FrontendFieldUiHint>,
    field_ui_meta: Option<FrontendFieldUiMeta>,
) -> ResolvedFrontendFieldUiMeta {
    let mut resolved = field_hint
        .map(ResolvedFrontendFieldUiMeta::from_hint)
        .unwrap_or_default();
    resolved.apply_dict_type_override(dict_type);
    if let Some(field_ui_meta) = field_ui_meta {
        resolved.apply_meta(field_ui_meta);
    }
    resolved
}

impl SelectedFieldSummary {
    fn collect(
        search_fields: &[FrontendPageFieldContext],
        table_fields: &[FrontendPageFieldContext],
        form_fields: &[FrontendPageFieldContext],
    ) -> Self {
        let mut summary = Self {
            has_search_fields: !search_fields.is_empty(),
            ..Self::default()
        };
        summary.visit_search_fields(search_fields);
        summary.visit_table_fields(table_fields);
        summary.visit_form_fields(form_fields);
        summary
    }

    fn flags(&self) -> FrontendPageFlags {
        FrontendPageFlags {
            uses_table_dict: self.uses_table_dict,
            uses_search_dict: self.uses_search_dict,
            uses_form_dict: self.uses_form_dict,
            uses_table_tag: self.uses_table_tag,
            uses_table_image: self.uses_table_image,
            uses_form_upload: self.uses_form_upload,
            uses_form_editor: self.uses_form_editor,
            uses_avatar_placeholder: self.uses_avatar_placeholder,
            uses_range_search: self.uses_range_search,
            has_search_fields: self.has_search_fields,
        }
    }

    fn required_dict_types(&self) -> Vec<String> {
        self.required_dict_types.iter().cloned().collect()
    }

    fn visit_search_fields(&mut self, fields: &[FrontendPageFieldContext]) {
        for field in fields {
            self.collect_dict_type(field);
            self.uses_search_dict |= field.dict_type.is_some();
            self.uses_range_search |= field.search_exclude_param;
            self.uses_avatar_placeholder |= field.semantic_kind.is_avatar();
        }
    }

    fn visit_table_fields(&mut self, fields: &[FrontendPageFieldContext]) {
        for field in fields {
            self.collect_dict_type(field);
            self.uses_table_dict |= field.dict_type.is_some();
            self.uses_table_tag |= matches!(
                field.table_display,
                TableDisplayKind::BooleanTag
                    | TableDisplayKind::DictTag
                    | TableDisplayKind::LocalTag
            );
            self.uses_table_image |= matches!(field.table_display, TableDisplayKind::Image);
            self.uses_avatar_placeholder |= field.semantic_kind.is_avatar();
        }
    }

    fn visit_form_fields(&mut self, fields: &[FrontendPageFieldContext]) {
        for field in fields {
            self.collect_dict_type(field);
            self.uses_form_dict |= field.dict_type.is_some();
            self.uses_form_upload |= matches!(
                field.form_widget,
                FormWidgetKind::ImageUpload | FormWidgetKind::FileUpload
            );
            self.uses_form_editor |= matches!(field.form_widget, FormWidgetKind::Editor);
            self.uses_avatar_placeholder |= field.semantic_kind.is_avatar();
        }
    }

    fn collect_dict_type(&mut self, field: &FrontendPageFieldContext) {
        if let Some(dict_type) = &field.dict_type {
            self.required_dict_types.insert(dict_type.clone());
        }
    }
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
        for (field, meta) in &request.field_ui_meta {
            ensure_valid_identifier(field, "field_ui_meta field")?;
            if meta
                .dict_type
                .as_deref()
                .is_some_and(|dict_type| dict_type.trim().is_empty())
            {
                return Err(McpError::invalid_params(
                    format!("field_ui_meta dict_type for field `{field}` cannot be empty"),
                    None,
                ));
            }
            for (segment_label, props) in [
                (
                    "search.props",
                    meta.search
                        .as_ref()
                        .and_then(|segment| segment.props.as_ref()),
                ),
                (
                    "form.props",
                    meta.form
                        .as_ref()
                        .and_then(|segment| segment.props.as_ref()),
                ),
            ] {
                if let Some(props) = props {
                    if !props.is_object() {
                        return Err(McpError::invalid_params(
                            format!(
                                "field_ui_meta {segment_label} for field `{field}` must be a JSON object"
                            ),
                            None,
                        ));
                    }
                }
            }
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
        .filter_map(|field| field_map.get(&field.name))
        .filter(|field| field.search_visible)
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
        .filter_map(|field| field_map.get(&field.name))
        .map(|field| field.name.clone())
        .collect::<BTreeSet<_>>();
    let table_fields = select_field_contexts(
        "table_fields",
        &field_map,
        request.table_fields.as_deref(),
        &allowed_table_fields,
        default_table_field_names(&crud_context, &field_map),
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

    let default_form_fields = default_form_field_names(&crud_context, &field_map);
    let allowed_form_fields = default_form_fields.iter().cloned().collect::<BTreeSet<_>>();
    let form_fields = select_field_contexts(
        "form_fields",
        &field_map,
        request.form_fields.as_deref(),
        &allowed_form_fields,
        default_form_fields,
    )?;
    if form_fields.is_empty() {
        return Err(McpError::invalid_params(
            format!(
                "table `{}` does not have create/update fields suitable for a frontend form",
                table.name
            ),
            None,
        ));
    }

    let form_container = FrontendFormContainerContext::new(&form_fields);
    let form_fields = apply_form_container_layout(form_fields, &form_container);
    let selected_field_summary =
        SelectedFieldSummary::collect(&search_fields, &table_fields, &form_fields);
    let required_dict_types = selected_field_summary.required_dict_types();
    let page = build_page_naming_context(&crud_context, &table_fields);
    let flags = selected_field_summary.flags();

    Ok(FrontendPageTemplateContext {
        table,
        names,
        primary_key,
        page,
        form_container,
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
    ensure_known_request_fields(
        &schema.table,
        &known_fields,
        "dict binding field",
        request.dict_bindings.keys(),
    )?;
    ensure_known_request_fields(
        &schema.table,
        &known_fields,
        "field hint",
        request.field_hints.keys(),
    )?;
    ensure_known_request_fields(
        &schema.table,
        &known_fields,
        "field_ui_meta",
        request.field_ui_meta.keys(),
    )?;

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
            let resolved_ui_meta = resolve_frontend_field_ui_meta(
                request.dict_bindings.get(&field.name).cloned(),
                request.field_hints.get(&field.name).cloned(),
                request.field_ui_meta.get(&field.name).cloned(),
            );
            Ok((
                field.name.clone(),
                build_frontend_field_context(
                    field,
                    column,
                    create_fields.contains(field.name.as_str()),
                    update_fields.contains(field.name.as_str()),
                    resolved_ui_meta,
                    api,
                ),
            ))
        })
        .collect()
}

fn ensure_known_request_fields<'a>(
    table: &str,
    known_fields: &BTreeSet<&str>,
    label: &str,
    fields: impl IntoIterator<Item = &'a String>,
) -> Result<(), McpError> {
    for field in fields {
        if !known_fields.contains(field.as_str()) {
            return Err(McpError::invalid_params(
                format!("{label} `{field}` does not exist on table `{table}`"),
                None,
            ));
        }
    }
    Ok(())
}

fn build_frontend_field_context(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
    create_enabled: bool,
    update_enabled: bool,
    ui_meta: ResolvedFrontendFieldUiMeta,
    api: &FrontendPageApiContext,
) -> FrontendPageFieldContext {
    let semantic_signals = FieldSemanticSignals::from_field(field, column);
    let option_context = resolve_field_option_context(field, column, &ui_meta);
    let ui_components = resolve_field_ui_components(
        field,
        create_enabled,
        update_enabled,
        &ui_meta,
        &option_context,
        &semantic_signals,
    );
    let search_binding = resolve_search_binding(field, ui_components.search_widget);
    let form_default_value = default_form_value(
        field,
        column,
        option_context.dict_type.as_deref(),
        option_context.has_enum_options,
    );
    let form_required = resolve_form_required(field, column, create_enabled, &ui_meta);
    let table_width = suggested_table_width(field, ui_components.semantic_kind);
    let table_min_width = if table_width.is_some() {
        None
    } else {
        suggested_table_min_width(field, ui_components.semantic_kind, &semantic_signals)
    };
    let resolved_label = normalize_ui_label(&field.label);
    let search_placeholder = ui_meta.search_placeholder.unwrap_or_else(|| {
        field_placeholder("请选择", "请输入", field, ui_components.search_widget)
    });
    let form_placeholder = ui_meta.form_placeholder.unwrap_or_else(|| {
        field_placeholder(
            "请选择",
            "请输入",
            field,
            ui_components.form_widget.default_search_widget(),
        )
    });
    let search_props_literal = ui_meta.search_props.as_ref().map(render_js_literal);
    let form_props_literal = ui_meta.form_props.as_ref().map(render_js_literal);
    let form_upload_props_literal = form_props_literal.clone();
    let form_upload_config = ui_components.semantic_kind.upload_config();

    FrontendPageFieldContext {
        name: field.name.clone(),
        camel_name: field.camel_name.clone(),
        pascal_name: field.pascal_name.clone(),
        label: resolved_label,
        semantic_kind: ui_components.semantic_kind,
        type_info: field.type_info.clone(),
        dict_type: option_context.dict_type,
        local_options_literal: option_context.local_options_literal,
        option_items_literal: option_context.option_items_literal,
        search_widget: ui_components.search_widget,
        search_visible: ui_components.search_visible,
        search_model_key: search_binding.model_key,
        search_exclude_param: search_binding.param_start.is_some(),
        search_param_start: search_binding.param_start,
        search_param_end: search_binding.param_end,
        search_value_format: ui_components
            .search_widget
            .value_format()
            .map(ToOwned::to_owned),
        form_widget: ui_components.form_widget,
        form_visibility: ui_components.form_visibility,
        form_span: None,
        table_display: ui_components.table_display,
        table_width,
        table_min_width,
        table_sortable: is_sortable_field(field),
        table_overflow: should_enable_overflow(
            field,
            ui_components.semantic_kind,
            &semantic_signals,
        ),
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
        search_placeholder,
        search_props_literal,
        form_placeholder,
        form_props_literal,
        form_upload_accept: form_upload_config.and_then(|config| config.accept.map(str::to_string)),
        form_upload_button_text: form_upload_config.map(|config| config.button_text.to_string()),
        form_upload_props_literal,
        form_rule_trigger: ui_components.form_widget.rule_trigger().to_string(),
        bool_true_label: option_context.bool_true_label,
        bool_false_label: option_context.bool_false_label,
    }
}

fn resolve_field_option_context(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
    ui_meta: &ResolvedFrontendFieldUiMeta,
) -> ResolvedFieldOptionContext {
    let local_options = local_options(field, column);
    let has_enum_options = !local_options.is_empty();
    let dict_type = ui_meta.dict_type.clone();
    let bool_true_label = boolean_true_label(field);
    let bool_false_label = boolean_false_label(field);
    let local_options_literal = render_local_options_literal(&local_options);
    let option_items_literal = render_option_items_literal(
        dict_type.as_deref(),
        field.type_info.value_kind,
        local_options_literal.as_deref(),
        &bool_true_label,
        &bool_false_label,
    );
    ResolvedFieldOptionContext {
        dict_type,
        local_options_literal,
        option_items_literal,
        has_enum_options,
        bool_true_label,
        bool_false_label,
    }
}

fn resolve_field_ui_components(
    field: &FieldGenerationContext,
    create_enabled: bool,
    update_enabled: bool,
    ui_meta: &ResolvedFrontendFieldUiMeta,
    option_context: &ResolvedFieldOptionContext,
    semantic_signals: &FieldSemanticSignals,
) -> ResolvedFieldUiComponents {
    let semantic_kind = ui_meta
        .semantic
        .unwrap_or_else(|| infer_field_semantic(semantic_signals));
    let supports_selection = supports_selection(
        option_context.dict_type.is_some(),
        option_context.has_enum_options,
        field.type_info.value_kind,
    );
    let uses_range_search = !semantic_kind.blocks_range_search()
        && matches!(
            field.type_info.value_kind,
            FieldValueKind::Date | FieldValueKind::DateTime
        );
    let search_widget = ui_meta.search_widget.unwrap_or_else(|| {
        infer_search_widget(
            field.type_info.value_kind,
            option_context.has_enum_options,
            supports_selection,
            uses_range_search,
            semantic_signals,
        )
    });
    let form_widget = ui_meta.form_widget.unwrap_or_else(|| {
        infer_form_widget(
            field.type_info.value_kind,
            semantic_kind,
            option_context.dict_type.is_some(),
            option_context.has_enum_options,
            semantic_signals,
        )
    });
    let table_display = ui_meta.table_display.unwrap_or_else(|| {
        infer_table_display(
            field.type_info.value_kind,
            semantic_kind,
            option_context.dict_type.is_some(),
            option_context.has_enum_options,
        )
    });
    let search_visible = ui_meta.search_visible.unwrap_or_else(|| {
        !semantic_kind.hides_search()
            && !matches!(
                field.type_info.value_kind,
                FieldValueKind::Json | FieldValueKind::Other | FieldValueKind::Uuid
            )
    });

    ResolvedFieldUiComponents {
        semantic_kind,
        search_widget,
        search_visible,
        form_widget,
        form_visibility: resolve_form_visibility(create_enabled, update_enabled, semantic_kind),
        table_display,
    }
}

fn resolve_form_visibility(
    create_enabled: bool,
    update_enabled: bool,
    semantic_kind: FieldSemanticKind,
) -> FormFieldVisibility {
    match (create_enabled, update_enabled) {
        _ if semantic_kind == FieldSemanticKind::Password => FormFieldVisibility::AddOnly,
        (true, true) => FormFieldVisibility::Always,
        (true, false) => FormFieldVisibility::AddOnly,
        (false, true) => FormFieldVisibility::EditOnly,
        (false, false) => FormFieldVisibility::Always,
    }
}

fn resolve_search_binding(
    field: &FieldGenerationContext,
    search_widget: SearchWidgetKind,
) -> ResolvedSearchBinding {
    let (model_key, param_start, param_end) = infer_search_binding(field, search_widget);
    ResolvedSearchBinding {
        model_key,
        param_start,
        param_end,
    }
}

fn resolve_form_required(
    field: &FieldGenerationContext,
    column: &TableColumnSchema,
    create_enabled: bool,
    ui_meta: &ResolvedFrontendFieldUiMeta,
) -> bool {
    let inferred_form_required = create_enabled
        && !column.nullable
        && column.default_value.is_none()
        && !field.nullable_entity;
    ui_meta.form_required.unwrap_or(inferred_form_required)
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
    let mut candidates = filtered_field_contexts(
        crud_context.query_fields.iter(),
        field_map,
        |field, mapped_field| {
            field.name != crud_context.primary_key.name
                && !should_skip_default_search_field(field)
                && mapped_field.search_visible
        },
    );
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

fn default_table_field_names(
    crud_context: &CrudGenerationContext,
    field_map: &BTreeMap<String, FrontendPageFieldContext>,
) -> Vec<String> {
    filtered_field_names(crud_context.read_fields.iter(), field_map, |field, _| {
        !matches!(field.type_info.value_kind, FieldValueKind::Json)
            && !should_skip_default_table_field(field)
    })
}

fn default_form_field_names(
    crud_context: &CrudGenerationContext,
    field_map: &BTreeMap<String, FrontendPageFieldContext>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    filtered_field_names(
        crud_context
            .create_fields
            .iter()
            .chain(crud_context.update_fields.iter()),
        field_map,
        |field, mapped_field| {
            !matches!(field.type_info.value_kind, FieldValueKind::Json)
                && !should_skip_default_form_field(field)
                && seen.insert(mapped_field.name.clone())
        },
    )
}

fn filtered_field_contexts<'a>(
    fields: impl IntoIterator<Item = &'a FieldGenerationContext>,
    field_map: &'a BTreeMap<String, FrontendPageFieldContext>,
    mut include: impl FnMut(&FieldGenerationContext, &'a FrontendPageFieldContext) -> bool,
) -> Vec<&'a FrontendPageFieldContext> {
    fields
        .into_iter()
        .filter_map(|field| {
            let mapped_field = field_map.get(&field.name)?;
            include(field, mapped_field).then_some(mapped_field)
        })
        .collect()
}

fn filtered_field_names<'a>(
    fields: impl IntoIterator<Item = &'a FieldGenerationContext>,
    field_map: &'a BTreeMap<String, FrontendPageFieldContext>,
    include: impl FnMut(&FieldGenerationContext, &'a FrontendPageFieldContext) -> bool,
) -> Vec<String> {
    filtered_field_contexts(fields, field_map, include)
        .into_iter()
        .map(|field| field.name.clone())
        .collect()
}
fn ensure_unique_search_model_keys(fields: &[FrontendPageFieldContext]) -> Result<(), McpError> {
    let mut model_key_to_field = BTreeMap::<String, String>::new();
    for field in fields {
        if let Some(previous) =
            model_key_to_field.insert(field.search_model_key.clone(), field.name.clone())
        {
            return Err(McpError::invalid_params(
                format!(
                    "search fields `{previous}` and `{}` both map to search model key `{}`; choose only one or override the search component via field_ui_meta.search.component / field_hints.search_widget",
                    field.name, field.search_model_key
                ),
                None,
            ));
        }
    }
    Ok(())
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

fn apply_form_container_layout(
    form_fields: Vec<FrontendPageFieldContext>,
    form_container: &FrontendFormContainerContext,
) -> Vec<FrontendPageFieldContext> {
    form_fields
        .into_iter()
        .map(|mut field| {
            if field.form_span.is_none() {
                field.form_span = Some(field.form_widget.default_span(form_container.use_drawer));
            }
            field
        })
        .collect()
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

fn infer_field_semantic(signals: &FieldSemanticSignals) -> FieldSemanticKind {
    if signals.contains_any(PASSWORD_KEYWORDS) {
        return FieldSemanticKind::Password;
    }
    if signals.string_value && signals.contains_any(AVATAR_KEYWORDS) {
        return FieldSemanticKind::Avatar;
    }
    if signals.string_value && signals.contains_any(IMAGE_KEYWORDS) {
        return FieldSemanticKind::Image;
    }
    if signals.string_value && signals.contains_any(FILE_KEYWORDS) {
        return FieldSemanticKind::File;
    }
    if signals.string_value && signals.contains_any(ICON_KEYWORDS) {
        return FieldSemanticKind::Icon;
    }
    if signals.string_value && signals.contains_any(EMAIL_KEYWORDS) {
        return FieldSemanticKind::Email;
    }
    if signals.string_value && signals.contains_any(PHONE_KEYWORDS) {
        return FieldSemanticKind::Phone;
    }
    if signals.string_value && signals.contains_any(URL_KEYWORDS) {
        return FieldSemanticKind::Url;
    }
    if signals.textarea_like {
        return FieldSemanticKind::RichText;
    }
    FieldSemanticKind::Plain
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

fn supports_selection(
    has_dict_binding: bool,
    has_enum_options: bool,
    value_kind: FieldValueKind,
) -> bool {
    has_dict_binding || has_enum_options || value_kind == FieldValueKind::Boolean
}

fn infer_search_widget(
    value_kind: FieldValueKind,
    has_enum_options: bool,
    has_select_options: bool,
    uses_range_search: bool,
    semantic_signals: &FieldSemanticSignals,
) -> SearchWidgetKind {
    match value_kind {
        _ if has_enum_options && semantic_signals.gender_like => SearchWidgetKind::RadioGroup,
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
    }
}

fn infer_form_widget(
    value_kind: FieldValueKind,
    semantic_kind: FieldSemanticKind,
    has_dict_binding: bool,
    has_enum_options: bool,
    semantic_signals: &FieldSemanticSignals,
) -> FormWidgetKind {
    match value_kind {
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
            if !has_dict_binding && !has_enum_options =>
        {
            FormWidgetKind::InputNumber
        }
        FieldValueKind::Date => FormWidgetKind::Date,
        FieldValueKind::Time => FormWidgetKind::Time,
        FieldValueKind::DateTime => FormWidgetKind::DateTime,
        _ if has_dict_binding || has_enum_options => FormWidgetKind::Select,
        _ if semantic_signals.textarea_like => FormWidgetKind::Textarea,
        _ => FormWidgetKind::Input,
    }
}

fn infer_table_display(
    value_kind: FieldValueKind,
    semantic_kind: FieldSemanticKind,
    has_dict_binding: bool,
    has_enum_options: bool,
) -> TableDisplayKind {
    if let Some(display) = semantic_kind.default_table_display() {
        display
    } else if has_dict_binding {
        TableDisplayKind::DictTag
    } else if has_enum_options {
        TableDisplayKind::LocalTag
    } else if value_kind == FieldValueKind::Boolean {
        TableDisplayKind::BooleanTag
    } else {
        TableDisplayKind::Text
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
    if contains_any(&field.name, SEARCH_PRIORITY_KEYWORDS) {
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
        _ if semantic_kind.is_image_like() => Some(96),
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
    semantic_signals: &FieldSemanticSignals,
) -> Option<u16> {
    match field.type_info.value_kind {
        _ if semantic_kind == FieldSemanticKind::Url => Some(220),
        FieldValueKind::String => Some(if semantic_signals.textarea_like {
            180
        } else {
            140
        }),
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
    semantic_signals: &FieldSemanticSignals,
) -> bool {
    !(semantic_kind.is_image_like()
        || semantic_kind == FieldSemanticKind::Url
        || semantic_kind == FieldSemanticKind::Icon)
        && (semantic_signals.textarea_like
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
    dict_type: Option<&str>,
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

fn render_option_items_literal(
    dict_type: Option<&str>,
    value_kind: FieldValueKind,
    local_options_literal: Option<&str>,
    bool_true_label: &str,
    bool_false_label: &str,
) -> String {
    if let Some(dict_type) = dict_type {
        return render_dict_options_literal(dict_type, value_kind);
    }
    if let Some(local_options_literal) = local_options_literal {
        return local_options_literal.to_string();
    }
    if value_kind == FieldValueKind::Boolean {
        return render_boolean_options_literal(bool_true_label, bool_false_label);
    }
    "[]".to_string()
}

fn render_dict_options_literal(dict_type: &str, value_kind: FieldValueKind) -> String {
    format!(
        "getDict('{}').map((item) => ({{ label: item.label, value: {} }}))",
        escape_js_single_quoted_string(dict_type),
        dict_option_value_expression(value_kind)
    )
}

fn dict_option_value_expression(value_kind: FieldValueKind) -> &'static str {
    match value_kind {
        FieldValueKind::Integer | FieldValueKind::Float => "Number(item.value)",
        FieldValueKind::Boolean => "item.value === 'true'",
        _ => "item.value",
    }
}

fn render_boolean_options_literal(true_label: &str, false_label: &str) -> String {
    format!(
        "[{{ label: {:?}, value: true }}, {{ label: {:?}, value: false }}]",
        true_label, false_label
    )
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

fn field_placeholder(
    select_prefix: &str,
    input_prefix: &str,
    field: &FieldGenerationContext,
    widget: SearchWidgetKind,
) -> String {
    if widget.hides_placeholder() {
        String::new()
    } else if widget.uses_select_placeholder() {
        format!("{select_prefix}{}", normalize_ui_label(&field.label))
    } else {
        format!("{input_prefix}{}", normalize_ui_label(&field.label))
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_optional_object(value: Option<JsonValue>) -> Option<JsonValue> {
    value.and_then(|value| match value {
        JsonValue::Object(map) if !map.is_empty() => Some(JsonValue::Object(map)),
        JsonValue::Object(_) => None,
        other => Some(other),
    })
}

fn render_js_literal(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => format!("'{}'", escape_js_single_quoted_string(value)),
        JsonValue::Array(values) => {
            let items = values.iter().map(render_js_literal).collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        JsonValue::Object(map) => {
            let properties = map
                .iter()
                .map(|(key, value)| {
                    format!(
                        "'{}':{}",
                        escape_js_single_quoted_string(key),
                        render_js_literal(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", properties.join(","))
        }
    }
}

fn escape_js_single_quoted_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\'' => "\\'".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

fn contains_any(hint: &str, needles: &[&str]) -> bool {
    let hint = hint.to_ascii_lowercase();
    needles
        .iter()
        .any(|needle| hint.contains(&needle.to_ascii_lowercase()))
}

fn is_textarea_field_like_name(name: &str) -> bool {
    contains_any(name, TEXTAREA_FIELD_NAME_KEYWORDS)
}

fn should_skip_default_form_field(field: &FieldGenerationContext) -> bool {
    contains_any(field.name.as_str(), SKIP_DEFAULT_FORM_FIELD_KEYWORDS)
}

fn should_skip_default_search_field(field: &FieldGenerationContext) -> bool {
    contains_any(field.name.as_str(), SKIP_DEFAULT_SEARCH_FIELD_KEYWORDS)
}

fn should_skip_default_table_field(field: &FieldGenerationContext) -> bool {
    contains_any(field.name.as_str(), SKIP_DEFAULT_TABLE_FIELD_KEYWORDS)
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

    fn assert_no_triple_newlines(contents: &str) {
        assert!(
            !contents.contains("\n\n\n"),
            "generated source contains multiple consecutive blank lines:\n{contents}"
        );
    }

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

    fn sample_user_entity_source() -> &'static str {
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
"#
    }

    fn sample_page_request(schema: TableSchema) -> GenerateFrontendPageRequest {
        GenerateFrontendPageRequest {
            schema,
            overwrite: true,
            route_base: None,
            output_dir: None,
            target_preset: FrontendTargetPreset::SummerMcp,
            api_import_path: None,
            api_namespace: None,
            api_list_item_type_name: None,
            api_detail_type_name: None,
            dict_bindings: BTreeMap::new(),
            field_hints: BTreeMap::new(),
            field_ui_meta: BTreeMap::new(),
            search_fields: None,
            table_fields: None,
            form_fields: None,
        }
    }

    fn sample_crud_context(schema: &TableSchema) -> CrudGenerationContext {
        CrudGenerationContextBuilder::build_from_entity_source(
            schema.clone(),
            None,
            sample_user_entity_source(),
        )
        .unwrap()
    }

    #[test]
    fn field_ui_meta_overrides_hints_and_dict_bindings() {
        let schema = sample_user_schema();
        let crud_context = sample_crud_context(&schema);
        let api = FrontendPageApiContext::from_generated_contract(&crud_context);
        let mut request = sample_page_request(schema.clone());
        request.dict_bindings = BTreeMap::from([("status".to_string(), "user_status".to_string())]);
        request.field_hints = BTreeMap::from([
            (
                "status".to_string(),
                FrontendFieldUiHint {
                    search_widget: Some(SearchWidgetKind::Input),
                    form_widget: Some(FormWidgetKind::Input),
                    table_display: Some(TableDisplayKind::Text),
                    ..Default::default()
                },
            ),
            (
                "avatar".to_string(),
                FrontendFieldUiHint {
                    form_widget: Some(FormWidgetKind::FileUpload),
                    ..Default::default()
                },
            ),
        ]);
        request.field_ui_meta = BTreeMap::from([
            (
                "status".to_string(),
                FrontendFieldUiMeta {
                    dict_type: Some("showcase_status".to_string()),
                    search: Some(SearchFieldUiMeta {
                        component: Some(SearchWidgetKind::Select),
                        placeholder: Some("请选择业务状态".to_string()),
                        ..Default::default()
                    }),
                    form: Some(FormFieldUiMeta {
                        component: Some(FormWidgetKind::Select),
                        required: Some(false),
                        placeholder: Some("请选择业务状态".to_string()),
                        ..Default::default()
                    }),
                    table: Some(TableFieldUiMeta {
                        component: Some(TableDisplayKind::DictTag),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ),
            (
                "avatar".to_string(),
                FrontendFieldUiMeta {
                    form: Some(FormFieldUiMeta {
                        component: Some(FormWidgetKind::ImageUpload),
                        ..Default::default()
                    }),
                    table: Some(TableFieldUiMeta {
                        component: Some(TableDisplayKind::Image),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ),
        ]);

        let field_map = build_field_map(&schema, &crud_context, &request, &api).unwrap();
        let status = field_map.get("status").unwrap();
        assert_eq!(status.label, "用户状态");
        assert_eq!(status.dict_type.as_deref(), Some("showcase_status"));
        assert_eq!(status.search_widget, SearchWidgetKind::Select);
        assert_eq!(status.search_placeholder, "请选择业务状态");
        assert_eq!(status.form_widget, FormWidgetKind::Select);
        assert!(!status.form_required);
        assert_eq!(status.form_placeholder, "请选择业务状态");
        assert_eq!(status.table_display, TableDisplayKind::DictTag);

        let avatar = field_map.get("avatar").unwrap();
        assert_eq!(avatar.form_widget, FormWidgetKind::ImageUpload);
        assert_eq!(avatar.table_display, TableDisplayKind::Image);
    }

    #[test]
    fn field_hints_keep_legacy_search_visibility_override() {
        let schema = sample_user_schema();
        let crud_context = sample_crud_context(&schema);
        let mut request = sample_page_request(schema.clone());
        request.field_hints = BTreeMap::from([(
            "status".to_string(),
            FrontendFieldUiHint {
                search_visible: Some(false),
                ..Default::default()
            },
        )]);

        let template_context =
            build_template_context(request.clone(), crud_context.clone()).unwrap();
        assert!(
            !template_context
                .search_fields
                .iter()
                .any(|field| field.name == "status")
        );
        assert!(
            template_context
                .table_fields
                .iter()
                .any(|field| field.name == "status")
        );

        let api = FrontendPageApiContext::from_generated_contract(&crud_context);
        let field_map = build_field_map(&schema, &crud_context, &request, &api).unwrap();
        let status = field_map.get("status").unwrap();
        assert!(!status.search_visible);
    }

    #[tokio::test]
    async fn generator_renders_field_ui_props_into_templates() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-props-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_user.rs"),
            sample_user_entity_source(),
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
                field_hints: BTreeMap::new(),
                field_ui_meta: BTreeMap::from([
                    (
                        "user_name".to_string(),
                        FrontendFieldUiMeta {
                            search: Some(SearchFieldUiMeta {
                                props: Some(serde_json::json!({ "maxlength": 20 })),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                    (
                        "description".to_string(),
                        FrontendFieldUiMeta {
                            form: Some(FormFieldUiMeta {
                                props: Some(serde_json::json!({ "rows": 6 })),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                    (
                        "avatar".to_string(),
                        FrontendFieldUiMeta {
                            form: Some(FormFieldUiMeta {
                                props: Some(serde_json::json!({ "buttonText": "重新上传头像" })),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                ]),
                search_fields: Some(vec!["user_name".to_string(), "status".to_string()]),
                table_fields: Some(vec!["avatar".to_string(), "user_name".to_string()]),
                form_fields: Some(vec!["avatar".to_string(), "description".to_string()]),
            })
            .await
            .unwrap();

        let search_file = std::fs::read_to_string(&result.search_file).unwrap();
        let index_file = std::fs::read_to_string(&result.index_file).unwrap();
        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();

        assert!(search_file.contains("...{'maxlength':20}"));
        assert!(dialog_file.contains("...{'rows':6}"));
        assert!(dialog_file.contains(":span=\"24\""));
        assert!(dialog_file.contains("...{'buttonText':'重新上传头像'}"));
        assert!(index_file.contains("prop: 'avatar'"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_renders_supported_component_protocol_variants() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-components-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_user.rs"),
            sample_user_entity_source(),
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
                dict_bindings: BTreeMap::from([
                    ("status".to_string(), "user_status".to_string()),
                    ("gender".to_string(), "user_gender".to_string()),
                ]),
                field_hints: BTreeMap::new(),
                field_ui_meta: BTreeMap::from([
                    (
                        "status".to_string(),
                        FrontendFieldUiMeta {
                            search: Some(SearchFieldUiMeta {
                                component: Some(SearchWidgetKind::Cascader),
                                props: Some(serde_json::json!({
                                    "props": { "multiple": true }
                                })),
                                ..Default::default()
                            }),
                            form: Some(FormFieldUiMeta {
                                component: Some(FormWidgetKind::TreeSelect),
                                props: Some(serde_json::json!({
                                    "checkStrictly": true
                                })),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                    (
                        "gender".to_string(),
                        FrontendFieldUiMeta {
                            search: Some(SearchFieldUiMeta {
                                component: Some(SearchWidgetKind::CheckboxGroup),
                                ..Default::default()
                            }),
                            form: Some(FormFieldUiMeta {
                                component: Some(FormWidgetKind::RadioGroup),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                    (
                        "description".to_string(),
                        FrontendFieldUiMeta {
                            form: Some(FormFieldUiMeta {
                                component: Some(FormWidgetKind::Editor),
                                props: Some(serde_json::json!({
                                    "height": "320px"
                                })),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ),
                ]),
                search_fields: Some(vec!["status".to_string(), "gender".to_string()]),
                table_fields: Some(vec!["user_name".to_string(), "status".to_string()]),
                form_fields: Some(vec![
                    "status".to_string(),
                    "gender".to_string(),
                    "description".to_string(),
                ]),
            })
            .await
            .unwrap();

        let search_file = std::fs::read_to_string(&result.search_file).unwrap();
        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();

        assert!(search_file.contains("type: 'cascader'"));
        assert!(search_file.contains("type: 'checkboxgroup'"));
        assert!(search_file.contains("getDict('user_status').map((item) => ({"));
        assert!(dialog_file.contains("<ElTreeSelect"));
        assert!(dialog_file.contains("<ElRadioGroup"));
        assert!(dialog_file.contains("<ArtWangEditor"));
        assert!(dialog_file.contains("import ArtWangEditor"));
        assert!(dialog_file.contains("...{'height':'320px'}"));
        assert_no_triple_newlines(&search_file);
        assert_no_triple_newlines(&dialog_file);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_uses_drawer_for_dense_forms() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-drawer-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        std::fs::create_dir_all(root.join(MODEL_ENTITY_DIR)).unwrap();
        std::fs::write(
            root.join(MODEL_ENTITY_DIR).join("sys_profile.rs"),
            r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub profile_code: String,
    pub title: String,
    pub owner_name: String,
    pub owner_phone: String,
    pub owner_email: String,
    pub status: i16,
    pub priority: i32,
    pub launch_at: DateTime,
    pub description: Option<String>,
}
"#,
        )
        .unwrap();

        let schema = TableSchema {
            schema: "public".to_string(),
            table: "sys_profile".to_string(),
            comment: Some("档案".to_string()),
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
                    comment: Some("主键".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "profile_code".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("档案编码".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "title".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("标题".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "owner_name".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("负责人".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "owner_phone".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("联系电话".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "owner_email".to_string(),
                    pg_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("联系邮箱".to_string()),
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
                TableColumnSchema {
                    name: "priority".to_string(),
                    pg_type: "integer".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("优先级".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "launch_at".to_string(),
                    pg_type: "timestamp without time zone".to_string(),
                    nullable: false,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("上线时间".to_string()),
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
                    comment: Some("描述".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        };

        let generator = FrontendPageGenerator::with_workspace_root(root.clone());
        let result = generator
            .generate(GenerateFrontendPageRequest {
                schema,
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/views/system".to_string()),
                target_preset: FrontendTargetPreset::SummerMcp,
                api_import_path: None,
                api_namespace: None,
                api_list_item_type_name: None,
                api_detail_type_name: None,
                dict_bindings: BTreeMap::new(),
                field_hints: BTreeMap::new(),
                field_ui_meta: BTreeMap::new(),
                search_fields: None,
                table_fields: None,
                form_fields: None,
            })
            .await
            .unwrap();

        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();
        assert!(dialog_file.contains("<ElDrawer"));
        assert!(!dialog_file.contains("<ElDialog"));
        assert!(dialog_file.contains("size=\"760px\""));
        assert!(dialog_file.contains(":span=\"12\""));
        assert!(dialog_file.contains(":span=\"24\""));

        let _ = std::fs::remove_dir_all(&root);
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
                field_ui_meta: BTreeMap::new(),
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
        assert!(!index_file.contains("width: 96,\n          minWidth:"));
        assert_no_triple_newlines(&index_file);

        let search_file = std::fs::read_to_string(&result.search_file).unwrap();
        assert!(search_file.contains("getDict('user_status')"));
        assert!(search_file.contains("type: 'datetimerange'"));
        assert!(search_file.contains("type SearchFormModel = {"));
        assert!(search_file.contains("key: 'createTimeRange'"));
        assert!(search_file.contains("Api.User.UserSearchParams['createTimeStart']"));
        assert!(search_file.contains("Api.User.UserSearchParams['createTimeEnd']"));
        assert!(search_file.contains("valueFormat: 'YYYY-MM-DDTHH:mm:ss'"));
        assert_no_triple_newlines(&search_file);

        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();
        assert!(dialog_file.contains("fetchCreateUser"));
        assert!(dialog_file.contains("fetchGetUserDetail"));
        assert!(dialog_file.contains("fetchUpdateUser"));
        assert!(dialog_file.contains("type UserListItem = Api.User.UserVo"));
        assert!(dialog_file.contains("type UserListItemDetail = Api.User.UserDetailVo"));
        assert!(dialog_file.contains("type: 'textarea'"));
        assert!(dialog_file.contains("<ElDialog"));
        assert!(!dialog_file.contains("<ElDrawer"));
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
        assert_no_triple_newlines(&dialog_file);

        assert_eq!(result.required_dict_types, vec!["user_status".to_string()]);
        assert_eq!(result.api_import_path, "@/api/user");
        assert_eq!(result.api_namespace, "User");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn generator_avoids_blank_lines_between_generated_frontend_fields() {
        let root = std::env::temp_dir().join(format!(
            "summer-mcp-frontend-page-generator-format-{}",
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

#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub user_name: String,
    pub avatar: String,
    pub status: UserStatus,
    pub launch_at: DateTime,
    pub create_time: DateTime,
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
                            name: "avatar".to_string(),
                            pg_type: "character varying".to_string(),
                            nullable: false,
                            primary_key: false,
                            hidden_on_read: false,
                            writable_on_create: true,
                            writable_on_update: true,
                            default_value: None,
                            comment: Some("头像".to_string()),
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
                        TableColumnSchema {
                            name: "launch_at".to_string(),
                            pg_type: "timestamp without time zone".to_string(),
                            nullable: false,
                            primary_key: false,
                            hidden_on_read: false,
                            writable_on_create: true,
                            writable_on_update: true,
                            default_value: None,
                            comment: Some("上线时间".to_string()),
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
                    indexes: vec![],
                    foreign_keys: vec![],
                    check_constraints: vec![],
                },
                overwrite: true,
                route_base: None,
                output_dir: Some("generated/views/system".to_string()),
                target_preset: FrontendTargetPreset::SummerMcp,
                api_import_path: None,
                api_namespace: None,
                api_list_item_type_name: None,
                api_detail_type_name: None,
                dict_bindings: BTreeMap::from([("status".to_string(), "user_status".to_string())]),
                field_hints: BTreeMap::new(),
                field_ui_meta: BTreeMap::new(),
                search_fields: Some(vec![
                    "user_name".to_string(),
                    "status".to_string(),
                    "launch_at".to_string(),
                    "create_time".to_string(),
                ]),
                table_fields: Some(vec![
                    "user_name".to_string(),
                    "avatar".to_string(),
                    "status".to_string(),
                    "launch_at".to_string(),
                ]),
                form_fields: Some(vec![
                    "user_name".to_string(),
                    "avatar".to_string(),
                    "status".to_string(),
                    "launch_at".to_string(),
                ]),
            })
            .await
            .unwrap();

        let index_file = std::fs::read_to_string(&result.index_file).unwrap();
        let search_file = std::fs::read_to_string(&result.search_file).unwrap();
        let dialog_file = std::fs::read_to_string(&result.dialog_file).unwrap();

        assert!(!index_file.contains("userName: undefined,\n\n    status: undefined"));
        assert!(!search_file.contains(
            "userName?: Api.User.UserSearchParams['userName']\n\n    status?: Api.User.UserSearchParams['status']"
        ));
        assert!(!dialog_file.contains("userName: '',\n\n    avatar: ''"));
        assert!(search_file.contains("valueFormat: 'YYYY-MM-DDTHH:mm:ss'"));
        assert!(dialog_file.contains("valueFormat: 'YYYY-MM-DDTHH:mm:ss'"));
        assert_no_triple_newlines(&index_file);
        assert_no_triple_newlines(&search_file);
        assert_no_triple_newlines(&dialog_file);

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
                field_ui_meta: BTreeMap::new(),
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
                field_ui_meta: BTreeMap::new(),
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
