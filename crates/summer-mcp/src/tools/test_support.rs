use crate::table_tools::schema::{TableColumnSchema, TableSchema};

pub(crate) fn sample_role_schema() -> TableSchema {
    TableSchema {
        schema: "public".to_string(),
        table: "sys_role".to_string(),
        comment: Some("角色管理".to_string()),
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
                name: "role_name".to_string(),
                pg_type: "character varying(64)".to_string(),
                nullable: false,
                primary_key: false,
                hidden_on_read: false,
                writable_on_create: true,
                writable_on_update: true,
                default_value: None,
                comment: Some("角色名称".to_string()),
                is_identity: false,
                is_generated: false,
                enum_values: None,
            },
            TableColumnSchema {
                name: "enabled".to_string(),
                pg_type: "boolean".to_string(),
                nullable: false,
                primary_key: false,
                hidden_on_read: false,
                writable_on_create: true,
                writable_on_update: true,
                default_value: Some("true".to_string()),
                comment: Some("是否启用".to_string()),
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
    }
}

pub(crate) const SAMPLE_ROLE_ENTITY_SOURCE: &str = r#"
#[sea_orm::model]
pub struct Model {
    pub id: i64,
    pub role_name: String,
    pub enabled: bool,
    pub create_time: DateTime,
}
"#;
