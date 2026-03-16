use std::collections::BTreeMap;

use rmcp::{
    ClientHandler, ServiceExt,
    model::{CallToolRequestParams, ClientInfo, ReadResourceRequestParams, ResourceContents},
};
use sea_orm::{DatabaseConnection, DbBackend, MockDatabase, MockExecResult, Value};
use summer_mcp::{AdminMcpServer, config::McpConfig};

#[derive(Debug, Clone, Default)]
struct DummyClient;

impl ClientHandler for DummyClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

fn row<const N: usize>(pairs: [(&str, Value); N]) -> BTreeMap<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn empty_rows() -> Vec<BTreeMap<String, Value>> {
    vec![]
}

fn public_tables_rows() -> Vec<BTreeMap<String, Value>> {
    vec![
        row([("table_name", "sys_role".into())]),
        row([("table_name", "sys_user".into())]),
    ]
}

fn sys_role_schema_rows() -> Vec<BTreeMap<String, Value>> {
    vec![
        column_schema_row(
            "System roles",
            "id",
            "bigint",
            false,
            Some("nextval('sys_role_id_seq'::regclass)"),
            true,
            Some("Primary key"),
        ),
        column_schema_row(
            "System roles",
            "role_name",
            "character varying",
            false,
            None,
            false,
            Some("Role display name"),
        ),
        column_schema_row(
            "System roles",
            "role_code",
            "character varying",
            false,
            None,
            false,
            Some("Role code"),
        ),
        column_schema_row(
            "System roles",
            "description",
            "text",
            false,
            None,
            false,
            Some("Role description"),
        ),
        column_schema_row(
            "System roles",
            "enabled",
            "boolean",
            false,
            Some("true"),
            false,
            Some("Enabled flag"),
        ),
        column_schema_row(
            "System roles",
            "create_time",
            "timestamp without time zone",
            false,
            None,
            false,
            Some("Created at"),
        ),
        column_schema_row(
            "System roles",
            "update_time",
            "timestamp without time zone",
            false,
            None,
            false,
            Some("Updated at"),
        ),
    ]
}

fn sys_user_schema_rows() -> Vec<BTreeMap<String, Value>> {
    vec![
        column_schema_row(
            "System users",
            "id",
            "bigint",
            false,
            Some("nextval('sys_user_id_seq'::regclass)"),
            true,
            Some("Primary key"),
        ),
        column_schema_row(
            "System users",
            "user_name",
            "character varying",
            false,
            None,
            false,
            Some("Login name"),
        ),
        column_schema_row(
            "System users",
            "password",
            "character varying",
            false,
            None,
            false,
            Some("Password hash"),
        ),
        column_schema_row(
            "System users",
            "nick_name",
            "character varying",
            false,
            None,
            false,
            Some("Display name"),
        ),
        column_schema_row(
            "System users",
            "gender",
            "smallint",
            false,
            Some("0"),
            false,
            Some("Gender code"),
        ),
        column_schema_row(
            "System users",
            "phone",
            "character varying",
            false,
            None,
            false,
            Some("Phone number"),
        ),
        column_schema_row(
            "System users",
            "email",
            "character varying",
            false,
            None,
            false,
            Some("Email"),
        ),
        column_schema_row(
            "System users",
            "avatar",
            "character varying",
            false,
            None,
            false,
            Some("Avatar URL"),
        ),
        column_schema_row(
            "System users",
            "status",
            "smallint",
            false,
            Some("1"),
            false,
            Some("Status code"),
        ),
        column_schema_row(
            "System users",
            "create_by",
            "character varying",
            false,
            None,
            false,
            Some("Created by"),
        ),
        column_schema_row(
            "System users",
            "create_time",
            "timestamp without time zone",
            false,
            None,
            false,
            Some("Created at"),
        ),
        column_schema_row(
            "System users",
            "update_by",
            "character varying",
            false,
            None,
            false,
            Some("Updated by"),
        ),
        column_schema_row(
            "System users",
            "update_time",
            "timestamp without time zone",
            false,
            None,
            false,
            Some("Updated at"),
        ),
    ]
}

fn column_schema_row(
    table_comment: &str,
    column_name: &str,
    pg_type: &str,
    is_nullable: bool,
    column_default: Option<&str>,
    is_primary_key: bool,
    column_comment: Option<&str>,
) -> BTreeMap<String, Value> {
    row([
        ("table_comment", Some(table_comment.to_string()).into()),
        ("column_name", column_name.into()),
        ("pg_type", pg_type.into()),
        ("is_nullable", is_nullable.into()),
        ("column_default", column_default.map(str::to_string).into()),
        ("is_identity", false.into()),
        ("is_generated", false.into()),
        ("is_primary_key", is_primary_key.into()),
        ("column_comment", column_comment.map(str::to_string).into()),
        ("enum_values", Option::<serde_json::Value>::None.into()),
    ])
}

fn sys_user_index_rows() -> Vec<BTreeMap<String, Value>> {
    vec![
        row([
            ("index_name", "sys_user_pkey".into()),
            ("is_unique", true.into()),
            ("is_primary", true.into()),
            ("columns", serde_json::json!(["id"]).into()),
        ]),
        row([
            ("index_name", "uk_sys_user_user_name".into()),
            ("is_unique", true.into()),
            ("is_primary", false.into()),
            ("columns", serde_json::json!(["user_name"]).into()),
        ]),
    ]
}

fn sys_user_check_rows() -> Vec<BTreeMap<String, Value>> {
    vec![row([
        ("constraint_name", "ck_sys_user_status".into()),
        ("expression", "CHECK ((status >= 0))".into()),
    ])]
}

fn sys_role_query_row() -> BTreeMap<String, Value> {
    row([
        ("id", 1_i64.into()),
        ("role_name", "Administrator".into()),
        ("enabled", true.into()),
    ])
}

fn sys_user_get_row() -> BTreeMap<String, Value> {
    row([
        ("id", 11_i64.into()),
        ("user_name", "admin".into()),
        ("nick_name", "Administrator".into()),
        ("gender", 0_i16.into()),
        ("phone", "13800000000".into()),
        ("email", "admin@example.com".into()),
        ("avatar", "".into()),
        ("status", 1_i16.into()),
        ("create_by", "system".into()),
        ("create_time", "2026-03-14 12:00:00".into()),
        ("update_by", "system".into()),
        ("update_time", "2026-03-14 12:00:00".into()),
    ])
}

fn sys_role_full_row(
    id: i64,
    enabled: bool,
    role_name: &str,
    role_code: &str,
) -> BTreeMap<String, Value> {
    row([
        ("id", id.into()),
        ("role_name", role_name.into()),
        ("role_code", role_code.into()),
        ("description", "Default admin role".into()),
        ("enabled", enabled.into()),
        ("create_time", "2026-03-14 12:00:00".into()),
        ("update_time", "2026-03-14 12:00:00".into()),
    ])
}

fn append_sys_role_describe_for_crud(db: MockDatabase) -> MockDatabase {
    db.append_query_results([sys_role_schema_rows()])
}

fn append_sys_user_describe(db: MockDatabase) -> MockDatabase {
    db.append_query_results([sys_user_schema_rows()])
        .append_query_results([sys_user_index_rows()])
        .append_query_results([empty_rows()])
        .append_query_results([sys_user_check_rows()])
}

fn append_sys_user_describe_for_crud(db: MockDatabase) -> MockDatabase {
    db.append_query_results([sys_user_schema_rows()])
}

fn build_runtime_mock_db() -> DatabaseConnection {
    let db = MockDatabase::new(DbBackend::Postgres).append_query_results([public_tables_rows()]);

    let db = append_sys_user_describe(db);
    let db = append_sys_role_describe_for_crud(db)
        .append_query_results([[row([("total", 1_i64.into())])]])
        .append_query_results([[sys_role_query_row()]]);
    let db = append_sys_user_describe_for_crud(db).append_query_results([[sys_user_get_row()]]);
    let db = db.append_query_results([[sys_role_query_row()]]);
    let db = append_sys_role_describe_for_crud(db)
        .append_query_results([[sys_role_full_row(2, true, "Editor", "R_EDITOR")]]);
    let db = append_sys_role_describe_for_crud(db).append_query_results([[sys_role_full_row(
        1,
        false,
        "Administrator",
        "R_ADMIN",
    )]]);
    let db = append_sys_role_describe_for_crud(db)
        .append_query_results([[row([("deleted", 1_i32.into())])]])
        .append_exec_results([MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }]);

    db.into_connection()
}

#[tokio::test]
async fn runtime_table_tools_are_exposed_over_mcp()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let db = build_runtime_mock_db();
    let server = AdminMcpServer::new(&McpConfig::default(), db);
    let server_handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    let client = DummyClient.serve(client_transport).await?;

    let tools = client.list_all_tools().await?;
    assert_eq!(tools.len(), 18);
    assert!(tools.iter().any(|tool| tool.name == "server_capabilities"));
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_admin_module_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_frontend_api_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_frontend_page_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_frontend_bundle_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_entity_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "upgrade_entity_enums_from_table")
    );
    assert!(tools.iter().any(|tool| tool.name == "menu_tool"));
    assert!(tools.iter().any(|tool| tool.name == "dict_tool"));
    assert!(tools.iter().any(|tool| tool.name == "schema_list_tables"));
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "schema_describe_table")
    );
    assert!(tools.iter().any(|tool| tool.name == "sql_exec"));
    assert!(tools.iter().any(|tool| tool.name == "sql_query_readonly"));
    assert!(tools.iter().any(|tool| tool.name == "table_get"));
    assert!(tools.iter().any(|tool| tool.name == "table_query"));
    assert!(tools.iter().any(|tool| tool.name == "table_insert"));
    assert!(tools.iter().any(|tool| tool.name == "table_update"));
    assert!(tools.iter().any(|tool| tool.name == "table_delete"));

    let resources = client.list_all_resources().await?;
    assert!(
        resources
            .iter()
            .any(|resource| resource.uri == "schema://tables")
    );

    let tables_resource = client
        .read_resource(ReadResourceRequestParams::new("schema://tables"))
        .await?;
    let tables_text = match &tables_resource.contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text,
        other => panic!("expected text resource, got {other:?}"),
    };
    let tables_json: serde_json::Value = serde_json::from_str(tables_text)?;
    assert!(
        tables_json["tables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|table| table == "sys_role")
    );

    let describe_result = client
        .call_tool(
            CallToolRequestParams::new("schema_describe_table").with_arguments(
                serde_json::json!({ "table": "sys_user" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;
    let describe_json = describe_result
        .structured_content
        .expect("expected structured content from schema_describe_table");
    assert_eq!(describe_json["table"], serde_json::json!("sys_user"));
    assert_eq!(describe_json["comment"], serde_json::json!("System users"));
    assert_eq!(
        describe_json["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|column| column["name"] == "password")
            .unwrap()["hidden_on_read"],
        serde_json::json!(true)
    );
    assert_eq!(
        describe_json["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|column| column["name"] == "status")
            .unwrap()["default_value"],
        serde_json::json!("1")
    );
    assert_eq!(
        describe_json["indexes"][0]["name"],
        serde_json::json!("sys_user_pkey")
    );
    assert_eq!(
        describe_json["check_constraints"][0]["name"],
        serde_json::json!("ck_sys_user_status")
    );

    let query_result = client
        .call_tool(
            CallToolRequestParams::new("table_query").with_arguments(
                serde_json::json!({
                    "table": "sys_role",
                    "columns": ["id", "role_name", "enabled"],
                    "order_by": ["id asc"],
                    "limit": 10,
                    "offset": 0
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let query_json = query_result
        .structured_content
        .expect("expected structured content from table_query");
    assert_eq!(query_json["table"], serde_json::json!("sys_role"));
    assert_eq!(query_json["total"], serde_json::json!(1));
    assert_eq!(
        query_json["items"][0]["role_name"],
        serde_json::json!("Administrator")
    );

    let get_result = client
        .call_tool(
            CallToolRequestParams::new("table_get").with_arguments(
                serde_json::json!({
                    "table": "sys_user",
                    "key": { "id": 11 }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let get_json = get_result
        .structured_content
        .expect("expected structured content from table_get");
    assert_eq!(get_json["found"], serde_json::json!(true));
    assert_eq!(get_json["item"]["user_name"], serde_json::json!("admin"));
    assert!(get_json["item"].get("password").is_none());

    let readonly_sql_result = client
        .call_tool(
            CallToolRequestParams::new("sql_query_readonly").with_arguments(
                serde_json::json!({
                    "sql": "select id, role_name, enabled from sys_role where enabled = $1 order by id asc",
                    "params": [true],
                    "limit": 5
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let readonly_sql_json = readonly_sql_result
        .structured_content
        .expect("expected structured content from sql_query_readonly");
    assert_eq!(readonly_sql_json["row_count"], serde_json::json!(1));
    assert_eq!(readonly_sql_json["limit"], serde_json::json!(5));
    assert_eq!(
        readonly_sql_json["rows"][0]["role_name"],
        serde_json::json!("Administrator")
    );

    let exec_result = client
        .call_tool(
            CallToolRequestParams::new("sql_exec").with_arguments(
                serde_json::json!({
                    "sql": "update sys_role set enabled = $1 where id = $2",
                    "params": [true, 1]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let exec_json = exec_result
        .structured_content
        .expect("expected structured content from sql_exec");
    assert_eq!(exec_json["rows_affected"], serde_json::json!(1));

    let insert_result = client
        .call_tool(
            CallToolRequestParams::new("table_insert").with_arguments(
                serde_json::json!({
                    "table": "sys_role",
                    "values": {
                        "role_name": "Editor",
                        "role_code": "R_EDITOR",
                        "description": "Editor role",
                        "enabled": true
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let insert_json = insert_result
        .structured_content
        .expect("expected structured content from table_insert");
    assert_eq!(insert_json["found"], serde_json::json!(true));
    assert_eq!(insert_json["item"]["id"], serde_json::json!(2));

    let update_result = client
        .call_tool(
            CallToolRequestParams::new("table_update").with_arguments(
                serde_json::json!({
                    "table": "sys_role",
                    "key": { "id": 1 },
                    "values": { "enabled": false }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let update_json = update_result
        .structured_content
        .expect("expected structured content from table_update");
    assert_eq!(update_json["found"], serde_json::json!(true));
    assert_eq!(update_json["changed"], serde_json::json!(true));
    assert_eq!(update_json["item"]["enabled"], serde_json::json!(false));

    let delete_result = client
        .call_tool(
            CallToolRequestParams::new("table_delete").with_arguments(
                serde_json::json!({
                    "table": "sys_role",
                    "key": { "id": 1 }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    let delete_json = delete_result
        .structured_content
        .expect("expected structured content from table_delete");
    assert_eq!(delete_json["found"], serde_json::json!(true));
    assert_eq!(delete_json["deleted"], serde_json::json!(true));
    assert_eq!(delete_json["rows_affected"], serde_json::json!(1));

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}
