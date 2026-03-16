use std::collections::BTreeMap;

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use syn::{
    AngleBracketedGenericArguments, Expr, ExprLit, ExprUnary, Fields, GenericArgument, Item,
    ItemEnum, Lit, Meta, PathArguments, PathSegment, Type, TypePath, UnOp, parse_file,
};

use crate::table_tools::schema::{TableColumnSchema, TableSchema};

use super::{
    enum_semantics::{
        EnumDraftSpec, EnumSemanticSource, build_enum_draft, render_ts_option_union,
        resolve_field_enum,
    },
    support::default_route_base,
};

const CLIENT_SUBMIT_BLOCKED_FIELD_NAMES: &[&str] = &[
    "create_by",
    "update_by",
    "created_by",
    "updated_by",
    "create_time",
    "update_time",
    "created_at",
    "updated_at",
];

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrudGenerationContext {
    pub(crate) table: TableGenerationContext,
    pub(crate) names: CrudNamingContext,
    pub(crate) paths: CrudPathContext,
    pub(crate) flags: CrudGenerationFlags,
    pub(crate) enum_drafts: Vec<EnumDraftSpec>,
    pub(crate) primary_key: FieldGenerationContext,
    pub(crate) fields: Vec<FieldGenerationContext>,
    pub(crate) create_fields: Vec<FieldGenerationContext>,
    pub(crate) update_fields: Vec<FieldGenerationContext>,
    pub(crate) query_fields: Vec<FieldGenerationContext>,
    pub(crate) read_fields: Vec<FieldGenerationContext>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TableGenerationContext {
    pub(crate) schema: String,
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) subject_label: String,
    pub(crate) route_base: String,
    pub(crate) file_stem: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrudNamingContext {
    pub(crate) table_module: String,
    pub(crate) table_pascal: String,
    pub(crate) resource_pascal: String,
    pub(crate) service_module: String,
    pub(crate) service_struct: String,
    pub(crate) create_dto: String,
    pub(crate) update_dto: String,
    pub(crate) query_dto: String,
    pub(crate) vo: String,
    pub(crate) ts_namespace: String,
    pub(crate) api_function_base: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrudPathContext {
    pub(crate) base: String,
    pub(crate) list: String,
    pub(crate) detail: String,
    pub(crate) create: String,
    pub(crate) update: String,
    pub(crate) delete: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrudGenerationFlags {
    pub(crate) uses_datetime_format: bool,
    pub(crate) uses_date_format: bool,
    pub(crate) uses_time_format: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FieldGenerationContext {
    pub(crate) name: String,
    pub(crate) camel_name: String,
    pub(crate) pascal_name: String,
    pub(crate) label: String,
    pub(crate) comment_lines: Vec<String>,
    pub(crate) nullable_entity: bool,
    pub(crate) create_required: bool,
    pub(crate) query_filter_method: String,
    pub(crate) type_info: FieldTypeContext,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FieldTypeContext {
    pub(crate) rust_entity: String,
    pub(crate) rust_input: String,
    pub(crate) ts_read: String,
    pub(crate) ts_input: String,
    pub(crate) value_kind: FieldValueKind,
    pub(crate) string_like: bool,
    pub(crate) datetime_like: bool,
    pub(crate) enum_source: Option<EnumSemanticSource>,
    pub(crate) enum_entity_path: Option<String>,
    pub(crate) enum_options: Vec<EnumOptionContext>,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FieldValueKind {
    String,
    Boolean,
    Integer,
    Float,
    Decimal,
    Date,
    Time,
    DateTime,
    Uuid,
    Json,
    Enum,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EnumOptionContext {
    pub(crate) label: String,
    pub(crate) value_literal: String,
    pub(crate) value_kind: EnumOptionValueKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnumOptionValueKind {
    String,
    Number,
}

#[derive(Clone)]
struct ParsedEntitySource {
    model_fields: BTreeMap<String, Type>,
    enums: BTreeMap<String, ParsedEntityEnum>,
}

#[derive(Debug, Clone)]
struct ParsedEntityEnum {
    options: Vec<EnumOptionContext>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CrudGenerationContextBuilder;

impl CrudGenerationContextBuilder {
    pub(crate) fn build_from_entity_source(
        schema: TableSchema,
        route_base_override: Option<String>,
        entity_source: &str,
    ) -> Result<CrudGenerationContext, McpError> {
        let entity_source = parse_entity_source(entity_source)?;
        build_context(schema, route_base_override, &entity_source)
    }
}

fn build_context(
    schema: TableSchema,
    route_base_override: Option<String>,
    entity_source: &ParsedEntitySource,
) -> Result<CrudGenerationContext, McpError> {
    let primary_key_columns = schema.primary_key_columns();
    if primary_key_columns.len() != 1 {
        return Err(McpError::invalid_params(
            format!(
                "generate_admin_module_from_table currently supports single primary key tables only; table `{}` has primary key {:?}",
                schema.table, schema.primary_key
            ),
            None,
        ));
    }

    let route_base = route_base_override.unwrap_or_else(|| default_route_base(&schema.table));
    let table_pascal = snake_to_pascal(&schema.table);
    let resource_pascal = snake_to_pascal(&route_base);
    let file_stem = route_base.replace('_', "-");
    let table_label = schema
        .comment
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| schema.table.clone());
    let subject_label = strip_management_suffix(&table_label).to_string();
    let base_path = format!("/{route_base}");

    let names = CrudNamingContext {
        table_module: schema.table.clone(),
        table_pascal: table_pascal.clone(),
        resource_pascal: resource_pascal.clone(),
        service_module: format!("{}_service", schema.table),
        service_struct: format!("{table_pascal}Service"),
        create_dto: format!("Create{resource_pascal}Dto"),
        update_dto: format!("Update{resource_pascal}Dto"),
        query_dto: format!("{resource_pascal}QueryDto"),
        vo: format!("{resource_pascal}Vo"),
        ts_namespace: resource_pascal.clone(),
        api_function_base: resource_pascal,
    };

    let table = TableGenerationContext {
        schema: schema.schema.clone(),
        name: schema.table.clone(),
        label: table_label,
        subject_label,
        route_base: route_base.clone(),
        file_stem,
    };

    let fields = schema
        .columns
        .iter()
        .map(|column| build_generated_field(&table, &names, &schema.table, column, entity_source))
        .collect::<Result<Vec<_>, McpError>>()?;

    let enum_drafts = fields
        .iter()
        .filter_map(|field| {
            let column = schema.column(&field.name)?;
            build_enum_draft(
                &table,
                &names,
                &field.name,
                &field.label,
                column,
                field.type_info.value_kind,
                field.type_info.enum_source,
                field.type_info.enum_entity_path.as_deref(),
                &field.type_info.enum_options,
            )
        })
        .collect();

    let primary_key_name = primary_key_columns[0].name.as_str();
    let primary_key = fields
        .iter()
        .find(|field| field.name == primary_key_name)
        .cloned()
        .ok_or_else(|| {
            McpError::internal_error(
                format!(
                    "primary key field `{primary_key_name}` was not found in generation context"
                ),
                None,
            )
        })?;

    let create_fields = filter_fields_by_column(&fields, &schema, |column| {
        column.writable_on_create && !is_client_submit_blocked_field(&column.name)
    })
    .into_iter()
    .map(|mut field| {
        let column = schema
            .columns
            .iter()
            .find(|column| column.name == field.name)
            .expect("field must map back to schema column");
        field.create_required =
            !column.nullable && column.default_value.is_none() && !field.nullable_entity;
        field
    })
    .collect::<Vec<_>>();

    let update_fields = filter_fields_by_column(&fields, &schema, |column| {
        column.writable_on_update && !is_client_submit_blocked_field(&column.name)
    });

    let read_fields = filter_fields_by_column(&fields, &schema, |column| !column.hidden_on_read);
    let query_fields = read_fields.clone();

    let flags = CrudGenerationFlags {
        uses_datetime_format: read_fields
            .iter()
            .any(|field| field.type_info.datetime_like),
        uses_date_format: read_fields
            .iter()
            .any(|field| matches!(field.type_info.value_kind, FieldValueKind::Date)),
        uses_time_format: read_fields
            .iter()
            .any(|field| matches!(field.type_info.value_kind, FieldValueKind::Time)),
    };

    Ok(CrudGenerationContext {
        table,
        names,
        paths: CrudPathContext {
            list: format!("{base_path}/list"),
            detail: format!("{base_path}/{{id}}"),
            create: base_path.clone(),
            update: format!("{base_path}/{{id}}"),
            delete: format!("{base_path}/{{id}}"),
            base: base_path,
        },
        flags,
        enum_drafts,
        primary_key,
        fields,
        create_fields,
        update_fields,
        query_fields,
        read_fields,
    })
}

fn build_generated_field(
    _table: &TableGenerationContext,
    _names: &CrudNamingContext,
    table_module: &str,
    column: &TableColumnSchema,
    entity_source: &ParsedEntitySource,
) -> Result<FieldGenerationContext, McpError> {
    let raw_type = entity_source
        .model_fields
        .get(&column.name)
        .ok_or_else(|| {
            McpError::internal_error(
                format!(
                    "entity field `{}` was not found in crates/model/src/entity/{table_module}.rs",
                    column.name
                ),
                None,
            )
        })?;
    let nullable_entity = option_inner_type(raw_type).is_some();
    let rust_entity = rewrite_type_for_generated_code(raw_type, table_module)?;
    let rust_input = resolve_input_type(raw_type, table_module)?;
    let comment_lines = split_comment_lines(column.comment.as_deref());
    let value_kind = infer_value_kind(raw_type, &rust_input);
    let entity_enum_options = resolve_entity_enum_options(raw_type, value_kind, entity_source);
    let enum_resolution = resolve_field_enum(column, value_kind, entity_enum_options);
    let enum_source = enum_resolution.as_ref().map(|resolution| resolution.source);
    let enum_options = enum_resolution
        .map(|resolution| resolution.options)
        .unwrap_or_default();
    let enum_entity_path = (value_kind == FieldValueKind::Enum)
        .then(|| rust_input.clone())
        .filter(|path| path.starts_with("crate::entity::"));
    let ts_input = ts_type_for_input(value_kind, &rust_input, nullable_entity, &enum_options);
    let ts_read = ts_type_for_read(value_kind, &rust_entity, nullable_entity, &enum_options);
    Ok(FieldGenerationContext {
        name: column.name.clone(),
        camel_name: snake_to_camel(&column.name),
        pascal_name: snake_to_pascal(&column.name),
        label: comment_lines
            .first()
            .map(|line| normalize_field_label(line))
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| column.name.clone()),
        comment_lines,
        nullable_entity,
        create_required: false,
        query_filter_method: if is_string_type(&rust_input) {
            "contains".to_string()
        } else {
            "eq".to_string()
        },
        type_info: FieldTypeContext {
            string_like: is_string_type(&rust_input),
            datetime_like: is_datetime_type(&rust_input),
            rust_entity,
            rust_input,
            ts_read,
            ts_input,
            value_kind,
            enum_source,
            enum_entity_path,
            enum_options,
        },
    })
}

fn parse_entity_source(contents: &str) -> Result<ParsedEntitySource, McpError> {
    let file = parse_file(contents).map_err(|error| {
        McpError::internal_error(format!("parse entity file failed: {error}"), None)
    })?;

    let model = file.items.iter().find_map(|item| match item {
        Item::Struct(item) if item.ident == "Model" => Some(item),
        _ => None,
    });

    let Some(model) = model else {
        return Err(McpError::internal_error(
            "failed to find `pub struct Model` in generated entity file",
            None,
        ));
    };

    let Fields::Named(fields) = &model.fields else {
        return Err(McpError::internal_error(
            "entity `Model` must use named fields",
            None,
        ));
    };

    let mut result = BTreeMap::new();
    for field in &fields.named {
        let Some(ident) = &field.ident else {
            continue;
        };
        result.insert(ident.to_string(), field.ty.clone());
    }

    let enums = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(item) => parse_entity_enum(item),
            _ => None,
        })
        .collect();

    Ok(ParsedEntitySource {
        model_fields: result,
        enums,
    })
}

#[cfg(test)]
fn parse_entity_model_fields(contents: &str) -> Result<BTreeMap<String, Type>, McpError> {
    Ok(parse_entity_source(contents)?.model_fields)
}

fn parse_entity_enum(item: &ItemEnum) -> Option<(String, ParsedEntityEnum)> {
    let options = item
        .variants
        .iter()
        .filter_map(parse_entity_enum_variant)
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }

    Some((item.ident.to_string(), ParsedEntityEnum { options }))
}

fn parse_entity_enum_variant(variant: &syn::Variant) -> Option<EnumOptionContext> {
    let (value_literal, value_kind) = parse_variant_sea_orm_value(variant.attrs.as_slice())?;
    let label = doc_comment(variant.attrs.as_slice())
        .unwrap_or_else(|| split_pascal_words(&variant.ident.to_string()));
    Some(EnumOptionContext {
        label,
        value_literal,
        value_kind,
    })
}

fn parse_variant_sea_orm_value(attrs: &[syn::Attribute]) -> Option<(String, EnumOptionValueKind)> {
    for attr in attrs {
        let Meta::List(_) = &attr.meta else {
            continue;
        };
        if !attr.path().is_ident("sea_orm") {
            continue;
        }

        let mut result = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("num_value") {
                let expr = meta.value()?.parse::<Expr>()?;
                result =
                    parse_numeric_literal(&expr).map(|value| (value, EnumOptionValueKind::Number));
            } else if meta.path.is_ident("string_value") {
                let expr = meta.value()?.parse::<Expr>()?;
                result =
                    parse_string_literal(&expr).map(|value| (value, EnumOptionValueKind::String));
            }
            Ok(())
        });

        if result.is_some() {
            return result;
        }
    }

    None
}

fn parse_numeric_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(value),
            ..
        }) => Some(value.base10_digits().to_string()),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => parse_numeric_literal(expr).map(|value| format!("-{value}")),
        _ => None,
    }
}

fn parse_string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Some(format!("{:?}", value.value())),
        _ => None,
    }
}

fn resolve_entity_enum_options(
    raw_type: &Type,
    value_kind: FieldValueKind,
    entity_source: &ParsedEntitySource,
) -> Vec<EnumOptionContext> {
    if value_kind != FieldValueKind::Enum {
        return Vec::new();
    }

    root_type_ident(raw_type)
        .and_then(|ident| entity_source.enums.get(&ident))
        .map(|definition| definition.options.clone())
        .unwrap_or_default()
}

fn doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("doc") {
            return None;
        }
        let Meta::NameValue(meta) = &attr.meta else {
            return None;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) = &meta.value
        else {
            return None;
        };
        Some(value.value().trim().to_string())
    })
}

fn split_pascal_words(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .fold(String::new(), |mut output, (idx, ch)| {
            if idx > 0 && ch.is_ascii_uppercase() {
                output.push(' ');
            }
            output.push(ch);
            output
        })
}

fn rewrite_type_for_generated_code(
    raw_type: &Type,
    table_module: &str,
) -> Result<String, McpError> {
    match raw_type {
        Type::Path(type_path) => rewrite_type_path_for_generated_code(type_path, table_module),
        Type::Reference(type_ref) => {
            let lifetime = type_ref
                .lifetime
                .as_ref()
                .map(|lifetime| format!("{} ", lifetime))
                .unwrap_or_default();
            let mutability = if type_ref.mutability.is_some() {
                "mut "
            } else {
                ""
            };
            Ok(format!(
                "&{lifetime}{mutability}{}",
                rewrite_type_for_generated_code(&type_ref.elem, table_module)?
            ))
        }
        Type::Tuple(tuple) => {
            let parts = tuple
                .elems
                .iter()
                .map(|elem| rewrite_type_for_generated_code(elem, table_module))
                .collect::<Result<Vec<_>, _>>()?;
            if parts.len() == 1 {
                Ok(format!("({},)", parts[0]))
            } else {
                Ok(format!("({})", parts.join(", ")))
            }
        }
        _ => Err(unsupported_type_error(raw_type)),
    }
}

fn rewrite_type_path_for_generated_code(
    type_path: &TypePath,
    table_module: &str,
) -> Result<String, McpError> {
    if type_path.qself.is_some() {
        return Err(McpError::internal_error(
            "qself types are not supported in generated module templates",
            None,
        ));
    }

    if let Some(inner) = option_inner_type(&Type::Path(type_path.clone())) {
        return Ok(format!(
            "Option<{}>",
            rewrite_type_for_generated_code(inner, table_module)?
        ));
    }

    if type_path.path.segments.len() == 1 {
        let segment = &type_path.path.segments[0];
        let ident = segment.ident.to_string();
        if is_builtin_type(&ident) && matches!(segment.arguments, PathArguments::None) {
            return Ok(ident);
        }

        return match ident.as_str() {
            "DateTime" => Ok("chrono::NaiveDateTime".to_string()),
            "Date" => Ok("chrono::NaiveDate".to_string()),
            "Time" => Ok("chrono::NaiveTime".to_string()),
            "Uuid" => Ok("uuid::Uuid".to_string()),
            "Json" | "JsonValue" => Ok("serde_json::Value".to_string()),
            _ if matches!(segment.arguments, PathArguments::None) => {
                Ok(format!("crate::entity::{table_module}::{ident}"))
            }
            _ => render_path_segments(&type_path.path.segments, table_module),
        };
    }

    render_path_segments(&type_path.path.segments, table_module)
}

fn render_path_segments(
    segments: &syn::punctuated::Punctuated<PathSegment, syn::token::PathSep>,
    table_module: &str,
) -> Result<String, McpError> {
    segments
        .iter()
        .map(|segment| render_path_segment(segment, table_module))
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("::"))
}

fn render_path_segment(segment: &PathSegment, table_module: &str) -> Result<String, McpError> {
    Ok(format!(
        "{}{}",
        segment.ident,
        render_path_arguments(&segment.arguments, table_module)?
    ))
}

fn render_path_arguments(
    arguments: &PathArguments,
    table_module: &str,
) -> Result<String, McpError> {
    match arguments {
        PathArguments::None => Ok(String::new()),
        PathArguments::AngleBracketed(args) => render_angle_bracketed_arguments(args, table_module),
        PathArguments::Parenthesized(args) => {
            let inputs = args
                .inputs
                .iter()
                .map(|ty| rewrite_type_for_generated_code(ty, table_module))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            let output = match &args.output {
                syn::ReturnType::Default => String::new(),
                syn::ReturnType::Type(_, ty) => {
                    format!(" -> {}", rewrite_type_for_generated_code(ty, table_module)?)
                }
            };
            Ok(format!("({inputs}){output}"))
        }
    }
}

fn render_angle_bracketed_arguments(
    args: &AngleBracketedGenericArguments,
    table_module: &str,
) -> Result<String, McpError> {
    let parts = args
        .args
        .iter()
        .map(|arg| render_generic_argument(arg, table_module))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("<{}>", parts.join(", ")))
}

fn render_generic_argument(
    argument: &GenericArgument,
    table_module: &str,
) -> Result<String, McpError> {
    match argument {
        GenericArgument::Type(ty) => rewrite_type_for_generated_code(ty, table_module),
        GenericArgument::Lifetime(lifetime) => Ok(lifetime.to_string()),
        GenericArgument::AssocType(assoc) => Ok(format!(
            "{} = {}",
            assoc.ident,
            rewrite_type_for_generated_code(&assoc.ty, table_module)?
        )),
        _ => Err(McpError::internal_error(
            "unsupported generic argument in entity model type",
            None,
        )),
    }
}

fn resolve_input_type(raw_type: &Type, table_module: &str) -> Result<String, McpError> {
    match option_inner_type(raw_type) {
        Some(inner) => rewrite_type_for_generated_code(inner, table_module),
        None => rewrite_type_for_generated_code(raw_type, table_module),
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
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let first = arguments.args.first()?;
    let GenericArgument::Type(inner) = first else {
        return None;
    };
    Some(inner)
}

fn split_comment_lines(comment: Option<&str>) -> Vec<String> {
    comment
        .into_iter()
        .flat_map(|comment| comment.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn normalize_field_label(label: &str) -> String {
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
                || suffix.contains('=')
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
    for separator in ['，', ','] {
        if let Some((prefix, suffix)) = trimmed.split_once(separator) {
            let suffix = suffix.trim();
            if suffix.starts_with('当')
                || suffix.starts_with("用于")
                || suffix.starts_with("用来")
                || suffix.contains("对应")
                || suffix.contains("解析")
                || suffix.contains("维护")
                || suffix.contains("重置")
                || suffix.contains("回退")
                || suffix.contains("靠前")
                || suffix.contains("后续")
                || suffix.contains("如 ")
                || suffix.contains("如basic")
                || suffix.contains("value_type")
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

fn infer_value_kind(raw_type: &Type, rust_input: &str) -> FieldValueKind {
    let Some(raw_ident) = root_type_ident(raw_type) else {
        return infer_value_kind_from_rendered_type(rust_input);
    };

    match raw_ident.as_str() {
        "String" => FieldValueKind::String,
        "bool" => FieldValueKind::Boolean,
        "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        | "isize" => FieldValueKind::Integer,
        "f32" | "f64" => FieldValueKind::Float,
        "Decimal" | "BigDecimal" => FieldValueKind::Decimal,
        "Date" => FieldValueKind::Date,
        "Time" => FieldValueKind::Time,
        "DateTime" => FieldValueKind::DateTime,
        "Uuid" => FieldValueKind::Uuid,
        "Json" | "JsonValue" => FieldValueKind::Json,
        ident if is_probable_entity_enum(raw_type, ident) => FieldValueKind::Enum,
        _ => infer_value_kind_from_rendered_type(rust_input),
    }
}

fn infer_value_kind_from_rendered_type(rust_input: &str) -> FieldValueKind {
    match base_type_name(rust_input) {
        Some("String") => FieldValueKind::String,
        Some("bool") => FieldValueKind::Boolean,
        Some(
            "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "isize",
        ) => FieldValueKind::Integer,
        Some("f32" | "f64") => FieldValueKind::Float,
        Some("Decimal" | "BigDecimal") => FieldValueKind::Decimal,
        Some("NaiveDate") => FieldValueKind::Date,
        Some("NaiveTime") => FieldValueKind::Time,
        Some("NaiveDateTime") => FieldValueKind::DateTime,
        Some("Uuid") => FieldValueKind::Uuid,
        Some("Value") => FieldValueKind::Json,
        _ => FieldValueKind::Other,
    }
}

fn ts_type_for_input(
    value_kind: FieldValueKind,
    rust_type: &str,
    nullable: bool,
    enum_options: &[EnumOptionContext],
) -> String {
    let literal_union = render_ts_option_union(enum_options);
    let base = match value_kind {
        FieldValueKind::String
        | FieldValueKind::Date
        | FieldValueKind::Time
        | FieldValueKind::DateTime
        | FieldValueKind::Uuid
        | FieldValueKind::Decimal => literal_union
            .clone()
            .unwrap_or_else(|| "string".to_string()),
        FieldValueKind::Boolean => "boolean".to_string(),
        FieldValueKind::Integer | FieldValueKind::Float => literal_union
            .clone()
            .unwrap_or_else(|| "number".to_string()),
        FieldValueKind::Json => "unknown".to_string(),
        FieldValueKind::Enum => infer_ts_enum_type(rust_type, enum_options),
        FieldValueKind::Other => "unknown".to_string(),
    };
    apply_nullable_ts_suffix(base, nullable)
}

fn ts_type_for_read(
    value_kind: FieldValueKind,
    rust_type: &str,
    nullable: bool,
    enum_options: &[EnumOptionContext],
) -> String {
    let base = match value_kind {
        FieldValueKind::Date | FieldValueKind::Time | FieldValueKind::DateTime => {
            "string".to_string()
        }
        _ => ts_type_for_input(value_kind, rust_type, false, enum_options),
    };
    apply_nullable_ts_suffix(base, nullable)
}

fn apply_nullable_ts_suffix(base: String, nullable: bool) -> String {
    if !nullable || base == "unknown" {
        base
    } else {
        format!("{base} | null")
    }
}

fn is_client_submit_blocked_field(name: &str) -> bool {
    CLIENT_SUBMIT_BLOCKED_FIELD_NAMES.contains(&name)
}

fn filter_fields_by_column(
    fields: &[FieldGenerationContext],
    schema: &TableSchema,
    predicate: impl Fn(&TableColumnSchema) -> bool,
) -> Vec<FieldGenerationContext> {
    fields
        .iter()
        .filter(|field| {
            schema
                .columns
                .iter()
                .find(|column| column.name == field.name)
                .is_some_and(&predicate)
        })
        .cloned()
        .collect()
}

fn infer_ts_enum_type(rust_type: &str, enum_options: &[EnumOptionContext]) -> String {
    let _ = rust_type;
    render_ts_option_union(enum_options).unwrap_or_else(|| {
        match enum_options.first().map(|option| option.value_kind) {
            Some(EnumOptionValueKind::String) => "string".to_string(),
            _ => "number".to_string(),
        }
    })
}

fn root_type_ident(ty: &Type) -> Option<String> {
    let inner = option_inner_type(ty).unwrap_or(ty);
    let Type::Path(type_path) = inner else {
        return None;
    };
    if type_path.qself.is_some() {
        return None;
    }
    type_path
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn is_probable_entity_enum(raw_type: &Type, ident: &str) -> bool {
    let inner = option_inner_type(raw_type).unwrap_or(raw_type);
    let Type::Path(type_path) = inner else {
        return false;
    };
    if type_path.qself.is_some() || type_path.path.segments.len() != 1 {
        return false;
    }
    !is_builtin_type(ident)
        && !matches!(
            ident,
            "DateTime" | "Date" | "Time" | "Uuid" | "Json" | "JsonValue"
        )
        && matches!(type_path.path.segments[0].arguments, PathArguments::None)
}

fn is_builtin_type(ident: &str) -> bool {
    matches!(
        ident,
        "String"
            | "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "f32"
            | "f64"
            | "usize"
            | "isize"
            | "Decimal"
            | "BigDecimal"
    )
}

fn is_string_type(ty: &str) -> bool {
    base_type_name(ty) == Some("String")
}

fn is_datetime_type(ty: &str) -> bool {
    base_type_name(ty) == Some("NaiveDateTime")
}

fn base_type_name(ty: &str) -> Option<&str> {
    let without_generics = ty.split('<').next()?;
    without_generics.rsplit("::").next()
}

fn unsupported_type_error(ty: &Type) -> McpError {
    let kind = match ty {
        Type::Array(_) => "array",
        Type::BareFn(_) => "bare_fn",
        Type::Group(_) => "group",
        Type::ImplTrait(_) => "impl_trait",
        Type::Infer(_) => "infer",
        Type::Macro(_) => "macro",
        Type::Never(_) => "never",
        Type::Paren(_) => "paren",
        Type::Path(_) => "path",
        Type::Ptr(_) => "ptr",
        Type::Reference(_) => "reference",
        Type::Slice(_) => "slice",
        Type::TraitObject(_) => "trait_object",
        Type::Tuple(_) => "tuple",
        Type::Verbatim(_) => "verbatim",
        _ => "unknown",
    };
    McpError::internal_error(
        format!("unsupported entity model type in template generator: {kind}"),
        None,
    )
}

fn strip_management_suffix(label: &str) -> &str {
    label
        .strip_suffix("管理")
        .or_else(|| label.strip_suffix("信息表"))
        .or_else(|| label.strip_suffix("数据表"))
        .or_else(|| label.strip_suffix("表"))
        .unwrap_or(label)
}

fn snake_to_pascal(value: &str) -> String {
    value
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut result = String::new();
                    result.push(first.to_ascii_uppercase());
                    result.push_str(chars.as_str());
                    result
                }
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn snake_to_camel(value: &str) -> String {
    let mut segments = value.split('_').filter(|segment| !segment.is_empty());
    let Some(first) = segments.next() else {
        return String::new();
    };
    let mut result = first.to_string();
    for segment in segments {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.push_str(chars.as_str());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        table_tools::schema::TableColumnSchema,
        tools::test_support::{SAMPLE_ROLE_ENTITY_SOURCE, sample_role_schema},
    };

    #[test]
    fn build_context_exposes_frontend_friendly_metadata() {
        let context = CrudGenerationContextBuilder::build_from_entity_source(
            sample_role_schema(),
            None,
            SAMPLE_ROLE_ENTITY_SOURCE,
        )
        .unwrap();

        assert_eq!(context.table.route_base, "role");
        assert_eq!(context.names.service_struct, "SysRoleService");
        assert_eq!(context.paths.list, "/role/list");

        let role_name = context
            .fields
            .iter()
            .find(|field| field.name == "role_name")
            .unwrap();
        assert_eq!(role_name.camel_name, "roleName");
        assert_eq!(role_name.label, "角色名称");
        assert_eq!(role_name.type_info.value_kind, FieldValueKind::String);
        assert_eq!(role_name.query_filter_method, "contains");
    }

    #[test]
    fn parse_entity_model_fields_reads_model_fields() {
        let fields = parse_entity_model_fields(
            r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub status: UserStatus,
    pub create_time: DateTime,
    pub enabled: Option<bool>,
}
"#,
        )
        .unwrap();

        assert!(matches!(fields.get("id"), Some(Type::Path(_))));
        assert!(matches!(fields.get("status"), Some(Type::Path(_))));
        assert!(matches!(fields.get("create_time"), Some(Type::Path(_))));
        assert!(matches!(fields.get("enabled"), Some(Type::Path(_))));
    }

    #[test]
    fn rewrite_type_uses_ast_based_mapping() {
        let rewritten = rewrite_type_for_generated_code(
            &syn::parse_str::<Type>("Option<UserStatus>").unwrap(),
            "sys_user",
        )
        .unwrap();
        assert_eq!(rewritten, "Option<crate::entity::sys_user::UserStatus>");

        let rewritten = rewrite_type_for_generated_code(
            &syn::parse_str::<Type>("DateTime").unwrap(),
            "sys_user",
        )
        .unwrap();
        assert_eq!(rewritten, "chrono::NaiveDateTime");
    }

    #[test]
    fn build_context_blocks_audit_submit_fields_and_simplifies_nullable_unknown() {
        let schema = TableSchema {
            schema: "public".to_string(),
            table: "biz_article".to_string(),
            comment: Some("文章".to_string()),
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
                    name: "create_by".to_string(),
                    pg_type: "bigint".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("创建人".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "created_at".to_string(),
                    pg_type: "timestamp without time zone".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("创建时间".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "updated_at".to_string(),
                    pg_type: "timestamp without time zone".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("更新时间".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
                TableColumnSchema {
                    name: "metadata".to_string(),
                    pg_type: "jsonb".to_string(),
                    nullable: true,
                    primary_key: false,
                    hidden_on_read: false,
                    writable_on_create: true,
                    writable_on_update: true,
                    default_value: None,
                    comment: Some("元数据".to_string()),
                    is_identity: false,
                    is_generated: false,
                    enum_values: None,
                },
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        };
        let entity_source = r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub title: String,
    pub create_by: Option<i64>,
    pub created_at: Option<DateTime>,
    pub updated_at: Option<DateTime>,
    pub metadata: Option<serde_json::Value>,
}
"#;

        let context =
            CrudGenerationContextBuilder::build_from_entity_source(schema, None, entity_source)
                .unwrap();

        let create_field_names = context
            .create_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        let update_field_names = context
            .update_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(create_field_names, vec!["title", "metadata"]);
        assert_eq!(update_field_names, vec!["title", "metadata"]);

        let metadata = context
            .fields
            .iter()
            .find(|field| field.name == "metadata")
            .unwrap();
        assert_eq!(metadata.type_info.ts_input, "unknown");
        assert_eq!(metadata.type_info.ts_read, "unknown");
    }

    #[test]
    fn build_context_infers_comment_backed_enum_metadata_for_frontend() {
        let schema = TableSchema {
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
            ],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
        };
        let entity_source = r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub status: i16,
}
"#;

        let context =
            CrudGenerationContextBuilder::build_from_entity_source(schema, None, entity_source)
                .unwrap();

        let status = context
            .fields
            .iter()
            .find(|field| field.name == "status")
            .unwrap();
        assert_eq!(status.type_info.ts_input, "1 | 2 | 3");
        assert_eq!(status.type_info.enum_options.len(), 3);
        assert_eq!(context.enum_drafts.len(), 1);
        assert_eq!(context.enum_drafts[0].dict_type, "user_status");
        assert_eq!(context.enum_drafts[0].rust_enum_name, "UserStatus");
    }

    #[test]
    fn normalize_field_label_strips_enum_and_explanatory_suffixes() {
        assert_eq!(
            normalize_field_label("值类型：1=文本 2=数字 3=布尔"),
            "值类型"
        );
        assert_eq!(
            normalize_field_label(
                "候选项字典类型编码，当 value_type=5 时使用，对应 sys_dict_type.dict_type"
            ),
            "候选项字典类型编码"
        );
        assert_eq!(
            normalize_field_label("角色ID（关联 sys_role.id）"),
            "角色ID"
        );
    }
}
