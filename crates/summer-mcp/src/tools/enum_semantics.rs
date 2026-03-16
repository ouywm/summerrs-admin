use std::sync::OnceLock;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::table_tools::schema::TableColumnSchema;

use super::generation_context::{
    CrudNamingContext, EnumOptionContext, EnumOptionValueKind, FieldValueKind,
    TableGenerationContext,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnumSemanticSource {
    EntityEnum,
    ColumnComment,
    NativeDbEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EnumDraftOptionSpec {
    pub label: String,
    pub value: String,
    pub value_kind: EnumOptionValueKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EnumDraftSpec {
    pub field_name: String,
    pub field_label: String,
    pub source: EnumSemanticSource,
    pub rust_enum_name: String,
    pub rust_rs_type: String,
    pub existing_entity_path: Option<String>,
    pub dict_type: String,
    pub dict_name: String,
    pub options: Vec<EnumDraftOptionSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedFieldEnum {
    pub(crate) source: EnumSemanticSource,
    pub(crate) options: Vec<EnumOptionContext>,
}

pub(crate) fn resolve_field_enum(
    column: &TableColumnSchema,
    value_kind: FieldValueKind,
    entity_options: Vec<EnumOptionContext>,
) -> Option<ResolvedFieldEnum> {
    if !entity_options.is_empty() {
        return Some(ResolvedFieldEnum {
            source: EnumSemanticSource::EntityEnum,
            options: entity_options,
        });
    }

    if let Some(options) = column
        .comment
        .as_deref()
        .and_then(|comment| parse_comment_enum_options(comment, value_kind))
    {
        return Some(ResolvedFieldEnum {
            source: EnumSemanticSource::ColumnComment,
            options,
        });
    }

    let options = column
        .enum_values
        .as_ref()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| option_from_raw_value(value, value_kind))
                .collect::<Vec<_>>()
        })
        .filter(|options| !options.is_empty())?;

    Some(ResolvedFieldEnum {
        source: EnumSemanticSource::NativeDbEnum,
        options,
    })
}

pub(crate) fn build_enum_draft(
    table: &TableGenerationContext,
    names: &CrudNamingContext,
    field_name: &str,
    field_label: &str,
    column: &TableColumnSchema,
    value_kind: FieldValueKind,
    enum_source: Option<EnumSemanticSource>,
    enum_entity_path: Option<&str>,
    enum_options: &[EnumOptionContext],
) -> Option<EnumDraftSpec> {
    let source = enum_source?;
    if enum_options.is_empty() {
        return None;
    }

    Some(EnumDraftSpec {
        field_name: field_name.to_string(),
        field_label: field_label.to_string(),
        source,
        rust_enum_name: inferred_rust_enum_name(names, field_name, enum_entity_path),
        rust_rs_type: rust_rs_type_for_field(value_kind, column, enum_options),
        existing_entity_path: enum_entity_path.map(ToOwned::to_owned),
        dict_type: infer_dict_type_code(&table.route_base, field_name),
        dict_name: format!("{}{}", table.subject_label, field_label),
        options: enum_options
            .iter()
            .map(|option| EnumDraftOptionSpec {
                label: option.label.clone(),
                value: enum_option_value(option),
                value_kind: option.value_kind,
            })
            .collect(),
    })
}

pub(crate) fn infer_dict_type_code(route_base: &str, field_name: &str) -> String {
    let normalized_field = field_name.trim_matches('_');
    if normalized_field.starts_with(&format!("{route_base}_")) || normalized_field == route_base {
        normalized_field.to_string()
    } else {
        format!("{route_base}_{normalized_field}")
    }
}

pub(crate) fn enum_option_value(option: &EnumOptionContext) -> String {
    serde_json::from_str::<String>(&option.value_literal)
        .unwrap_or_else(|_| option.value_literal.clone())
}

pub(crate) fn render_ts_option_union(enum_options: &[EnumOptionContext]) -> Option<String> {
    if enum_options.is_empty() {
        return None;
    }

    Some(
        enum_options
            .iter()
            .map(|option| option.value_literal.clone())
            .collect::<Vec<_>>()
            .join(" | "),
    )
}

fn parse_comment_enum_options(
    comment: &str,
    value_kind: FieldValueKind,
) -> Option<Vec<EnumOptionContext>> {
    let matches = enum_token_regex()
        .captures_iter(comment)
        .filter_map(|capture| {
            let value = capture.name("value")?;
            let whole = capture.get(0)?;
            Some((
                value.as_str().trim().to_string(),
                whole.start(),
                whole.end(),
            ))
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return None;
    }

    let mut options = Vec::new();
    for (idx, (value, _start, label_start)) in matches.iter().enumerate() {
        let label_end = matches
            .get(idx + 1)
            .map(|(_, start, _)| *start)
            .unwrap_or(comment.len());
        let raw_label = comment[*label_start..label_end].trim();
        let label = trim_enum_label(raw_label);
        if label.is_empty() {
            continue;
        }
        let Some(option) = option_from_value_and_label(value, &label, value_kind) else {
            continue;
        };
        options.push(option);
    }

    (!options.is_empty()).then_some(options)
}

fn option_from_raw_value(value: &str, value_kind: FieldValueKind) -> Option<EnumOptionContext> {
    let label = value.trim();
    if label.is_empty() {
        return None;
    }
    option_from_value_and_label(label, label, value_kind)
}

fn option_from_value_and_label(
    raw_value: &str,
    raw_label: &str,
    value_kind: FieldValueKind,
) -> Option<EnumOptionContext> {
    let value = raw_value.trim();
    let label = raw_label.trim();
    if value.is_empty() || label.is_empty() {
        return None;
    }

    match value_kind {
        FieldValueKind::String | FieldValueKind::Uuid | FieldValueKind::Decimal => {
            Some(EnumOptionContext {
                label: label.to_string(),
                value_literal: format!("{value:?}"),
                value_kind: EnumOptionValueKind::String,
            })
        }
        FieldValueKind::Integer | FieldValueKind::Float | FieldValueKind::Enum => {
            if !is_numeric_token(value) {
                return None;
            }
            Some(EnumOptionContext {
                label: label.to_string(),
                value_literal: value.to_string(),
                value_kind: EnumOptionValueKind::Number,
            })
        }
        _ => None,
    }
}

fn trim_enum_label(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(|ch| matches!(ch, '(' | '（' | '[' | '【'))
        .trim_end_matches(|ch| matches!(ch, ',' | '，' | ';' | '；' | ')' | '）' | ']' | '】'))
        .trim()
        .to_string()
}

fn enum_token_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?x)
            (?:
                ^ | [\s,，;；(（:：]
            )
            (?P<value>-?\d+|[A-Za-z][A-Za-z0-9_]*)
            \s*
            (?:=|:|：|-|－)
            \s*
        ",
        )
        .expect("enum token regex must compile")
    })
}

fn is_numeric_token(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

fn inferred_rust_enum_name(
    names: &CrudNamingContext,
    field_name: &str,
    enum_entity_path: Option<&str>,
) -> String {
    if let Some(existing) = enum_entity_path
        .and_then(|path| path.rsplit("::").next())
        .filter(|segment| !segment.is_empty())
    {
        return existing.to_string();
    }

    let field_pascal = snake_to_pascal(field_name);
    if field_pascal.starts_with(&names.resource_pascal) {
        field_pascal
    } else if !field_name.contains('_') && is_generic_enum_field_name(field_name) {
        format!("{}{}", names.resource_pascal, field_pascal)
    } else {
        field_pascal
    }
}

fn is_generic_enum_field_name(field_name: &str) -> bool {
    matches!(
        field_name,
        "status" | "state" | "type" | "kind" | "level" | "mode" | "gender" | "sex"
    )
}

fn rust_rs_type_for_field(
    value_kind: FieldValueKind,
    column: &TableColumnSchema,
    enum_options: &[EnumOptionContext],
) -> String {
    match value_kind {
        FieldValueKind::String | FieldValueKind::Uuid | FieldValueKind::Decimal => {
            "String".to_string()
        }
        FieldValueKind::Boolean => "bool".to_string(),
        FieldValueKind::Integer => integer_rs_type_for_pg(&column.pg_type),
        FieldValueKind::Enum => match enum_options.first().map(|option| option.value_kind) {
            Some(EnumOptionValueKind::String) => "String".to_string(),
            _ => integer_rs_type_for_pg(&column.pg_type),
        },
        FieldValueKind::Float => {
            if column.pg_type.contains("real") {
                "f32".to_string()
            } else {
                "f64".to_string()
            }
        }
        FieldValueKind::Date => "chrono::NaiveDate".to_string(),
        FieldValueKind::Time => "chrono::NaiveTime".to_string(),
        FieldValueKind::DateTime => "chrono::NaiveDateTime".to_string(),
        FieldValueKind::Json => "serde_json::Value".to_string(),
        FieldValueKind::Other => "String".to_string(),
    }
}

fn integer_rs_type_for_pg(pg_type: &str) -> String {
    let normalized = pg_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "smallint" | "smallserial" => "i16".to_string(),
        "integer" | "int" | "serial" | "int4" => "i32".to_string(),
        "bigint" | "bigserial" | "int8" => "i64".to_string(),
        _ => "i32".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comment_enum_options_from_chinese_comment() {
        let options =
            parse_comment_enum_options("状态：1-启用 2-禁用 3-注销", FieldValueKind::Integer)
                .unwrap();

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].label, "启用");
        assert_eq!(options[0].value_literal, "1");
        assert_eq!(options[2].label, "注销");
        assert_eq!(options[2].value_literal, "3");
    }

    #[test]
    fn parse_comment_enum_options_supports_parenthesized_assignments() {
        let options =
            parse_comment_enum_options("登录状态（1=成功, 2=失败）", FieldValueKind::Integer)
                .unwrap();

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].label, "成功");
        assert_eq!(options[1].label, "失败");
    }

    #[test]
    fn infer_dict_type_code_avoids_duplicate_route_prefix() {
        assert_eq!(infer_dict_type_code("menu", "menu_type"), "menu_type");
        assert_eq!(infer_dict_type_code("user", "status"), "user_status");
    }

    #[test]
    fn render_ts_option_union_uses_literal_values() {
        let union = render_ts_option_union(&[
            EnumOptionContext {
                label: "启用".to_string(),
                value_literal: "1".to_string(),
                value_kind: EnumOptionValueKind::Number,
            },
            EnumOptionContext {
                label: "禁用".to_string(),
                value_literal: "2".to_string(),
                value_kind: EnumOptionValueKind::Number,
            },
        ])
        .unwrap();

        assert_eq!(union, "1 | 2");
    }

    #[test]
    fn inferred_rust_enum_name_only_prefixes_generic_field_names() {
        let names = CrudNamingContext {
            table_module: "biz_showcase_profile".to_string(),
            table_pascal: "BizShowcaseProfile".to_string(),
            resource_pascal: "ShowcaseProfile".to_string(),
            service_module: "biz_showcase_profile_service".to_string(),
            service_struct: "BizShowcaseProfileService".to_string(),
            create_dto: "CreateShowcaseProfileDto".to_string(),
            update_dto: "UpdateShowcaseProfileDto".to_string(),
            query_dto: "ShowcaseProfileQueryDto".to_string(),
            vo: "ShowcaseProfileVo".to_string(),
            ts_namespace: "ShowcaseProfile".to_string(),
            api_function_base: "ShowcaseProfile".to_string(),
        };

        assert_eq!(
            inferred_rust_enum_name(&names, "status", None),
            "ShowcaseProfileStatus"
        );
        assert_eq!(
            inferred_rust_enum_name(&names, "contact_gender", None),
            "ContactGender"
        );
    }
}
