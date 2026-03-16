#![cfg(feature = "standalone")]

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use rmcp::{
    ClientHandler, ServiceExt,
    model::{CallToolRequestParams, ClientInfo},
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
struct DummyClient;

const TEST_SHOWCASE_PROFILE_TABLE: &str = "__test__biz_showcase_profile";
const TEST_SHOWCASE_PROFILE_ROUTE_BASE: &str = "showcase_profile";

impl ClientHandler for DummyClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

fn smoke_database_url() -> String {
    std::env::var("SUMMER_MCP_DATABASE_URL")
        .expect("SUMMER_MCP_DATABASE_URL must be set for this smoke test")
}

fn smoke_art_design_pro_dir() -> PathBuf {
    std::env::var("SUMMER_MCP_ART_DESIGN_PRO_DIR")
        .map(PathBuf::from)
        .expect("SUMMER_MCP_ART_DESIGN_PRO_DIR must be set for this smoke test")
}

fn smoke_transport(database_url: String) -> std::io::Result<TokioChildProcess> {
    TokioChildProcess::new({
        let mut command = Command::new(env!("CARGO_BIN_EXE_summerrs-mcp"));
        command
            .arg("--database-url")
            .arg(database_url)
            .arg("--transport")
            .arg("stdio");
        command
    })
}

fn json_args(value: Value) -> serde_json::Map<String, Value> {
    value
        .as_object()
        .expect("tool arguments must be a JSON object")
        .clone()
}

fn create_showcase_profile_test_table_sql() -> String {
    let table = TEST_SHOWCASE_PROFILE_TABLE;
    format!(
        r#"DO $do$
BEGIN
  EXECUTE 'DROP TABLE IF EXISTS public.{table} CASCADE';
  EXECUTE $sql$
    CREATE TABLE public.{table} (
      id BIGSERIAL PRIMARY KEY,
      showcase_code VARCHAR(64) NOT NULL UNIQUE,
      title VARCHAR(120) NOT NULL,
      avatar VARCHAR(255),
      cover_image VARCHAR(255),
      contact_name VARCHAR(64),
      contact_gender SMALLINT NOT NULL DEFAULT 0,
      contact_phone VARCHAR(32),
      contact_email VARCHAR(128),
      official_url VARCHAR(255),
      status SMALLINT NOT NULL DEFAULT 1,
      featured BOOLEAN NOT NULL DEFAULT FALSE,
      priority INTEGER NOT NULL DEFAULT 0,
      score NUMERIC(10,2),
      publish_date DATE,
      launch_at TIMESTAMP WITHOUT TIME ZONE,
      service_time TIME WITHOUT TIME ZONE,
      attachment_url VARCHAR(255),
      description TEXT,
      extra_notes TEXT,
      metadata JSONB,
      created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
      updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
      CONSTRAINT {table}_status_check CHECK (status IN (1, 2, 3)),
      CONSTRAINT {table}_contact_gender_check CHECK (contact_gender IN (0, 1, 2))
    )
  $sql$;
  EXECUTE $$COMMENT ON TABLE public.{table} IS '展示档案'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.id IS '主键'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.showcase_code IS '展示编码'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.title IS '标题'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.avatar IS '头像'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.cover_image IS '封面图片'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.contact_name IS '联系人'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.contact_gender IS '联系人性别'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.contact_phone IS '联系电话'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.contact_email IS '联系邮箱'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.official_url IS '官网链接'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.status IS '状态'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.featured IS '推荐'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.priority IS '优先级'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.score IS '评分'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.publish_date IS '发布日期'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.launch_at IS '上线时间'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.service_time IS '服务时间'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.attachment_url IS '附件'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.description IS '描述'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.extra_notes IS '备注'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.metadata IS '元数据'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.created_at IS '创建时间'$$;
  EXECUTE $$COMMENT ON COLUMN public.{table}.updated_at IS '更新时间'$$;
END
$do$;"#
    )
}

fn drop_showcase_profile_test_table_sql() -> String {
    format!("DROP TABLE IF EXISTS public.{TEST_SHOWCASE_PROFILE_TABLE} CASCADE")
}

fn test_entity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../model/src/entity")
}

fn prepare_showcase_profile_test_entity() -> io::Result<PathBuf> {
    let entity_dir = test_entity_dir();
    let source = entity_dir.join("biz_showcase_profile.rs");
    let target = entity_dir.join(format!("{TEST_SHOWCASE_PROFILE_TABLE}.rs"));
    fs::copy(source, &target)?;
    Ok(target)
}

fn copy_dir_filtered(source: &Path, target: &Path, ignored_names: &[&str]) -> io::Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        if ignored_names
            .iter()
            .any(|ignored| *ignored == file_name_str.as_ref())
        {
            continue;
        }

        let source_path = entry.path();
        let target_path = target.join(&file_name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_filtered(&source_path, &target_path, ignored_names)?;
        } else if file_type.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path)?;
        } else if file_type.is_symlink() {
            let link_target = fs::read_link(&source_path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &target_path)?;
        }
    }

    Ok(())
}

async fn assert_command_success(
    mut command: Command,
    label: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = command.output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "{label} failed with status {}.\nstdout:\n{}\nstderr:\n{}",
        output.status.code().map_or_else(
            || "terminated by signal".to_string(),
            |code| code.to_string()
        ),
        stdout,
        stderr
    )
    .into())
}

async fn prepare_art_design_pro_preview(
    source_root: &Path,
    preview_root: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = tokio::fs::remove_dir_all(preview_root).await;
    copy_dir_filtered(source_root, preview_root, &["node_modules", "dist", ".git"])?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(
        source_root.join("node_modules"),
        preview_root.join("node_modules"),
    )?;

    tokio::fs::create_dir_all(preview_root.join("src/types/import")).await?;
    tokio::fs::write(
        preview_root.join("src/types/import/generated-smoke.d.ts"),
        "import 'pinia-plugin-persistedstate'\n",
    )
    .await?;
    tokio::fs::write(
        preview_root.join("tsconfig.generated-showcase.json"),
        r#"{
  "extends": "./tsconfig.json",
  "include": [
    "src/env.d.ts",
    "src/types/import/*.d.ts",
    "src/types/api/**/*.d.ts",
    "src/api/showcase-profile.ts",
    "src/views/system/showcase-profile/**/*.vue"
  ],
  "exclude": ["dist", "node_modules"]
}
"#,
    )
    .await?;

    Ok(())
}

fn contains_menu_path(items: &[Value], path: &str) -> bool {
    items.iter().any(|item| {
        item.get("path").and_then(Value::as_str) == Some(path)
            || item
                .get("children")
                .and_then(Value::as_array)
                .is_some_and(|children| contains_menu_path(children, path))
    })
}

fn contains_button_mark(items: &[Value], auth_mark: &str) -> bool {
    items.iter().any(|item| {
        item.get("meta")
            .and_then(|meta| meta.get("authList"))
            .and_then(Value::as_array)
            .is_some_and(|buttons| {
                buttons
                    .iter()
                    .any(|button| button.get("authMark").and_then(Value::as_str) == Some(auth_mark))
            })
            || item
                .get("children")
                .and_then(Value::as_array)
                .is_some_and(|children| contains_button_mark(children, auth_mark))
    })
}

#[tokio::test]
#[ignore = "requires SUMMER_MCP_DATABASE_URL and a local PostgreSQL instance"]
async fn standalone_binary_serves_real_runtime_table_tools()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let transport = smoke_transport(smoke_database_url())?;

    let client = DummyClient::default().serve(transport).await?;

    let tools = client.list_all_tools().await?;
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_admin_module_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_entity_from_table")
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_frontend_bundle_from_table")
    );
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

    let query_result = client
        .call_tool(
            CallToolRequestParams::new("table_query").with_arguments(
                json!({
                    "table": "sys_role",
                    "columns": ["id", "enabled"],
                    "order_by": ["id asc"],
                    "limit": 1,
                    "offset": 0
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;

    let query_json = query_result
        .structured_content
        .expect("expected structured content from table_query");
    let first_role = query_json["items"]
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .expect("expected at least one sys_role row in the real database");

    let role_id = first_role["id"]
        .as_i64()
        .expect("expected integer sys_role id");
    let enabled = first_role["enabled"]
        .as_bool()
        .expect("expected boolean sys_role enabled field");

    let get_result = client
        .call_tool(
            CallToolRequestParams::new("table_get").with_arguments(
                json!({
                    "table": "sys_role",
                    "key": { "id": role_id }
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let get_json = get_result
        .structured_content
        .expect("expected structured content from table_get");
    assert_eq!(get_json["found"], serde_json::json!(true));
    assert_eq!(get_json["item"]["id"], serde_json::json!(role_id));

    let readonly_sql_result = client
        .call_tool(
            CallToolRequestParams::new("sql_query_readonly").with_arguments(
                json!({
                    "sql": "select id, enabled from sys_role order by id asc",
                    "limit": 1
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let readonly_sql_json = readonly_sql_result
        .structured_content
        .expect("expected structured content from sql_query_readonly");
    assert_eq!(readonly_sql_json["row_count"], serde_json::json!(1));
    assert_eq!(
        readonly_sql_json["rows"][0]["id"],
        serde_json::json!(role_id)
    );

    let exec_result = client
        .call_tool(
            CallToolRequestParams::new("sql_exec").with_arguments(
                json!({
                    "sql": "update sys_role set enabled = $1 where id = $2",
                    "params": [enabled, role_id]
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let exec_json = exec_result
        .structured_content
        .expect("expected structured content from sql_exec");
    assert_eq!(exec_json["rows_affected"], serde_json::json!(1));

    let update_result = client
        .call_tool(
            CallToolRequestParams::new("table_update").with_arguments(
                json!({
                    "table": "sys_role",
                    "key": { "id": role_id },
                    "values": { "enabled": enabled }
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let update_json = update_result
        .structured_content
        .expect("expected structured content from table_update");
    assert_eq!(update_json["found"], serde_json::json!(true));
    assert_eq!(update_json["item"]["enabled"], serde_json::json!(enabled));

    let output_dir = std::env::temp_dir().join(format!(
        "summer-mcp-generate-entity-test-{}",
        std::process::id()
    ));
    let generate_result = client
        .call_tool(
            CallToolRequestParams::new("generate_entity_from_table").with_arguments(
                json!({
                    "table": "sys_role",
                    "overwrite": true,
                    "output_dir": output_dir.display().to_string()
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let generate_json = generate_result
        .structured_content
        .expect("expected structured content from generate_entity_from_table");
    assert_eq!(generate_json["table"], serde_json::json!("sys_role"));

    let entity_file = output_dir.join("sys_role.rs");
    let entity_contents = tokio::fs::read_to_string(&entity_file).await?;
    assert!(entity_contents.contains("#[sea_orm(table_name = \"sys_role\")]"));
    assert!(!entity_contents.contains("schema_name = \"public\""));

    let mod_contents = tokio::fs::read_to_string(output_dir.join("mod.rs")).await?;
    assert!(mod_contents.contains("pub mod sys_role;"));

    let _ = tokio::fs::remove_dir_all(&output_dir).await;

    let admin_output_dir = std::env::temp_dir().join(format!(
        "summer-mcp-generate-admin-test-{}",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&admin_output_dir).await;

    let admin_generate_result = client
        .call_tool(
            CallToolRequestParams::new("generate_admin_module_from_table").with_arguments(
                json!({
                    "table": "sys_role",
                    "overwrite": true,
                    "output_dir": admin_output_dir.display().to_string()
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;
    let admin_generate_json = admin_generate_result
        .structured_content
        .expect("expected structured content from generate_admin_module_from_table");
    assert_eq!(admin_generate_json["table"], serde_json::json!("sys_role"));
    assert_eq!(admin_generate_json["route_base"], serde_json::json!("role"));

    let router_contents =
        tokio::fs::read_to_string(admin_output_dir.join("router/sys_role.rs")).await?;
    let service_contents =
        tokio::fs::read_to_string(admin_output_dir.join("service/sys_role_service.rs")).await?;
    let dto_contents = tokio::fs::read_to_string(admin_output_dir.join("dto/sys_role.rs")).await?;
    let vo_contents = tokio::fs::read_to_string(admin_output_dir.join("vo/sys_role.rs")).await?;
    let router_mod_contents =
        tokio::fs::read_to_string(admin_output_dir.join("router/mod.rs")).await?;

    assert!(router_contents.contains("pub async fn list("));
    assert!(service_contents.contains("pub struct SysRoleService"));
    assert!(dto_contents.contains("pub struct CreateRoleDto"));
    assert!(vo_contents.contains("pub struct RoleVo"));
    assert!(router_mod_contents.contains("pub mod sys_role;"));

    let _ = tokio::fs::remove_dir_all(&admin_output_dir).await;

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SUMMER_MCP_DATABASE_URL and a local PostgreSQL instance"]
async fn standalone_binary_generates_real_frontend_bundle_to_temp_dir()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let transport = smoke_transport(smoke_database_url())?;

    let client = DummyClient::default().serve(transport).await?;

    let output_dir = std::env::temp_dir().join("summer-mcp-frontend-bundle-real");
    let _ = tokio::fs::remove_dir_all(&output_dir).await;

    let generate_result = client
        .call_tool(
            CallToolRequestParams::new("generate_frontend_bundle_from_table").with_arguments(
                json!({
                    "table": "sys_user",
                    "overwrite": true,
                    "output_dir": output_dir.display().to_string(),
                    "dict_bindings": {
                        "status": "user_status"
                    },
                    "field_hints": {
                        "email": {
                            "table_display": "link"
                        }
                    }
                })
                .as_object()
                .expect("tool arguments must be a JSON object")
                .clone(),
            ),
        )
        .await?;

    let generate_json = generate_result
        .structured_content
        .expect("expected structured content from generate_frontend_bundle_from_table");
    assert_eq!(generate_json["table"], serde_json::json!("sys_user"));
    assert_eq!(generate_json["route_base"], serde_json::json!("user"));
    assert_eq!(
        generate_json["api_import_path"],
        serde_json::json!("@/api/user")
    );
    assert_eq!(generate_json["api_namespace"], serde_json::json!("User"));
    assert_eq!(
        generate_json["menu_config_draft"]["menus"][0]["path"],
        serde_json::json!("user")
    );
    assert_eq!(
        generate_json["menu_config_draft"]["menus"][0]["component"],
        serde_json::json!("/system/user")
    );
    assert!(
        generate_json["dict_bundle_drafts"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        generate_json["required_dict_types"]
            .as_array()
            .is_some_and(|items| items.iter().any(|value| value == "user_status"))
    );

    let api_file = output_dir.join("api/user.ts");
    let api_type_file = output_dir.join("api_type/user.d.ts");
    let page_dir = output_dir.join("views/system/user");
    let index_file = page_dir.join("index.vue");
    let search_file = page_dir.join("modules/user-search.vue");
    let dialog_file = page_dir.join("modules/user-dialog.vue");

    let api_contents = tokio::fs::read_to_string(&api_file).await?;
    let api_type_contents = tokio::fs::read_to_string(&api_type_file).await?;
    let index_contents = tokio::fs::read_to_string(&index_file).await?;
    let search_contents = tokio::fs::read_to_string(&search_file).await?;
    let dialog_contents = tokio::fs::read_to_string(&dialog_file).await?;

    assert!(api_contents.contains("fetchGetUserList"));
    assert!(api_contents.contains("fetchGetUserDetail"));
    assert!(api_type_contents.contains("namespace User"));
    assert!(api_type_contents.contains("interface UserDetailVo"));
    assert!(api_type_contents.contains("createTimeStart?: string"));
    assert!(api_type_contents.contains("createTimeEnd?: string"));

    assert!(index_contents.contains("UserSearch"));
    assert!(index_contents.contains("UserDialog"));
    assert!(index_contents.contains("fetchGetUserList"));
    assert!(index_contents.contains("fetchDeleteUser"));
    assert!(index_contents.contains("ElImage"));
    assert!(index_contents.contains("mailto:"));
    assert!(index_contents.contains("getDictLabel('user_status'"));

    assert!(search_contents.contains("ArtSearchBar"));
    assert!(search_contents.contains("getDict('user_status')"));
    assert!(search_contents.contains("getDict('user_gender')"));
    assert!(search_contents.contains("type: 'datetimerange'"));
    assert!(search_contents.contains("key: 'createTimeRange'"));

    assert!(dialog_contents.contains("fetchCreateUser"));
    assert!(dialog_contents.contains("fetchGetUserDetail"));
    assert!(dialog_contents.contains("fetchUpdateUser"));
    assert!(dialog_contents.contains("from '@/api/user'"));
    assert!(dialog_contents.contains("userName"));
    assert!(dialog_contents.contains("password"));
    assert!(dialog_contents.contains("ArtFileUpload"));
    assert!(dialog_contents.contains("handleAvatarUploadSuccess"));
    assert!(dialog_contents.contains("type UserListItem = Api.User.UserVo"));
    assert!(dialog_contents.contains("type UserListItemDetail = Api.User.UserDetailVo"));
    assert!(dialog_contents.contains("getDict('user_gender')"));
    assert!(dialog_contents.contains("defaultAvatar"));

    let dict_drafts = generate_json["dict_bundle_drafts"]
        .as_array()
        .expect("expected dict_bundle_drafts array");
    let first_dict_draft = dict_drafts
        .first()
        .cloned()
        .expect("expected at least one dict bundle draft");
    let dict_plan_result = client
        .call_tool(
            CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                "action": "plan_bundle",
                "bundle": first_dict_draft
            }))),
        )
        .await?;
    let dict_plan_json = dict_plan_result
        .structured_content
        .expect("expected structured content from dict_tool plan_bundle");
    assert_eq!(dict_plan_json["result"]["kind"], json!("bundle_sync"));

    let menu_plan_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "plan_config",
                "config": {
                    "menus": [
                        {
                            "name": "__mcp_bundle_user",
                            "path": "__mcp_bundle_user",
                            "component": "system/mcp-bundle-user/index",
                            "title": "MCP Bundle 用户",
                            "keepAlive": true,
                            "sort": 0,
                            "enabled": true,
                            "buttons": generate_json["menu_config_draft"]["menus"][0]["buttons"].clone()
                        }
                    ]
                }
            }))),
        )
        .await?;
    let menu_plan_json = menu_plan_result
        .structured_content
        .expect("expected structured content from menu_tool plan_config");
    assert_eq!(menu_plan_json["result"]["kind"], json!("config_sync"));

    println!("generated frontend bundle dir: {}", output_dir.display());

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SUMMER_MCP_DATABASE_URL and a local PostgreSQL instance"]
async fn standalone_binary_serves_real_menu_and_dict_config_actions()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let transport = smoke_transport(smoke_database_url())?;
    let client = DummyClient::default().serve(transport).await?;

    let dict_type = "__mcp_smoke_status";
    let root_path = "__mcp_smoke";
    let child_path = "runtime_config";
    let root_name = "__mcp_smoke_root";
    let child_name = "__mcp_smoke_config";
    let auth_mark = "mcp:smoke:config:view";

    let dict_bundle = json!({
        "dictName": "MCP 冒烟状态",
        "dictType": dict_type,
        "items": [
            {
                "dictLabel": "启用",
                "dictValue": "enabled",
                "dictSort": 0,
                "listClass": "success",
                "isDefault": true
            },
            {
                "dictLabel": "禁用",
                "dictValue": "disabled",
                "dictSort": 1,
                "listClass": "danger",
                "isDefault": false
            }
        ]
    });

    let menu_config = json!({
        "menus": [
            {
                "name": root_name,
                "path": root_path,
                "component": "layout/routerView/parent",
                "title": "MCP 冒烟",
                "icon": "i-mdi-test-tube",
                "sort": 9900,
                "enabled": true,
                "children": [
                    {
                        "name": child_name,
                        "path": child_path,
                        "component": "system/mcp-smoke/index",
                        "title": "运行时配置",
                        "sort": 1,
                        "enabled": true,
                        "buttons": [
                            {
                                "authName": "查看",
                                "authMark": auth_mark,
                                "sort": 1,
                                "enabled": true
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let dict_apply_result = client
        .call_tool(
            CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                "action": "apply_bundle",
                "operator": "codex_smoke",
                "bundle": dict_bundle
            }))),
        )
        .await?;
    let dict_apply_json = dict_apply_result
        .structured_content
        .expect("expected structured content from dict_tool apply_bundle");
    assert_eq!(dict_apply_json["result"]["kind"], json!("bundle_sync"));
    assert_eq!(
        dict_apply_json["result"]["sync"]["dictType"],
        json!(dict_type)
    );

    let dict_plan_result = client
        .call_tool(
            CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                "action": "plan_bundle",
                "bundle": {
                    "dictName": "MCP 冒烟状态",
                    "dictType": dict_type,
                    "items": [
                        {
                            "dictLabel": "启用",
                            "dictValue": "enabled",
                            "dictSort": 0,
                            "listClass": "success",
                            "isDefault": true
                        },
                        {
                            "dictLabel": "禁用",
                            "dictValue": "disabled",
                            "dictSort": 1,
                            "listClass": "danger",
                            "isDefault": false
                        }
                    ]
                }
            }))),
        )
        .await?;
    let dict_plan_json = dict_plan_result
        .structured_content
        .expect("expected structured content from dict_tool plan_bundle");
    assert_eq!(
        dict_plan_json["result"]["sync"]["plan"]["summary"]["createCount"],
        json!(0)
    );
    assert_eq!(
        dict_plan_json["result"]["sync"]["plan"]["summary"]["updateCount"],
        json!(0)
    );

    let export_output_dir = std::env::temp_dir().join("summer-mcp-menu-dict-export");
    let _ = tokio::fs::remove_dir_all(&export_output_dir).await;

    let dict_export_result = client
        .call_tool(
            CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                "action": "export_bundle",
                "output_dir": export_output_dir.display().to_string(),
                "bundle": {
                    "dictName": "MCP 冒烟状态",
                    "dictType": dict_type,
                    "items": [
                        {
                            "dictLabel": "启用",
                            "dictValue": "enabled",
                            "dictSort": 0,
                            "listClass": "success",
                            "isDefault": true
                        },
                        {
                            "dictLabel": "禁用",
                            "dictValue": "disabled",
                            "dictSort": 1,
                            "listClass": "danger",
                            "isDefault": false
                        }
                    ]
                }
            }))),
        )
        .await?;
    let dict_export_json = dict_export_result
        .structured_content
        .expect("expected structured content from dict_tool export_bundle");
    assert_eq!(dict_export_json["result"]["kind"], json!("bundle_export"));
    let dict_spec_file = dict_export_json["result"]["export"]["spec_file"]
        .as_str()
        .expect("expected dict export specFile");
    let dict_plan_file = dict_export_json["result"]["export"]["plan_file"]
        .as_str()
        .expect("expected dict export planFile");
    let dict_exported_spec = tokio::fs::read_to_string(dict_spec_file).await?;
    let dict_exported_plan = tokio::fs::read_to_string(dict_plan_file).await?;
    assert!(dict_exported_spec.contains(dict_type));
    assert!(dict_exported_plan.contains("\"createCount\": 0"));

    let dict_get_result = client
        .call_tool(
            CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                "action": "get_by_type",
                "dict_type": dict_type
            }))),
        )
        .await?;
    let dict_get_json = dict_get_result
        .structured_content
        .expect("expected structured content from dict_tool get_by_type");
    let dict_items = dict_get_json["result"]["items"]
        .as_array()
        .expect("expected dict items array");
    assert!(
        dict_items
            .iter()
            .any(|item| item["value"] == json!("enabled"))
    );
    assert!(
        dict_items
            .iter()
            .any(|item| item["value"] == json!("disabled"))
    );

    let menu_apply_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "apply_config",
                "config": menu_config
            }))),
        )
        .await?;
    let menu_apply_json = menu_apply_result
        .structured_content
        .expect("expected structured content from menu_tool apply_config");
    assert_eq!(menu_apply_json["result"]["kind"], json!("config_sync"));

    let menu_plan_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "plan_config",
                "config": {
                    "menus": [
                        {
                            "name": root_name,
                            "path": root_path,
                            "component": "layout/routerView/parent",
                            "title": "MCP 冒烟",
                            "icon": "i-mdi-test-tube",
                            "sort": 9900,
                            "enabled": true,
                            "children": [
                                {
                                    "name": child_name,
                                    "path": child_path,
                                    "component": "system/mcp-smoke/index",
                                    "title": "运行时配置",
                                    "sort": 1,
                                    "enabled": true,
                                    "buttons": [
                                        {
                                            "authName": "查看",
                                            "authMark": auth_mark,
                                            "sort": 1,
                                            "enabled": true
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            }))),
        )
        .await?;
    let menu_plan_json = menu_plan_result
        .structured_content
        .expect("expected structured content from menu_tool plan_config");
    assert_eq!(
        menu_plan_json["result"]["sync"]["plan"]["summary"]["createCount"],
        json!(0)
    );
    assert_eq!(
        menu_plan_json["result"]["sync"]["plan"]["summary"]["updateCount"],
        json!(0)
    );

    let menu_export_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "export_config",
                "output_dir": export_output_dir.display().to_string(),
                "config": {
                    "menus": [
                        {
                            "name": root_name,
                            "path": root_path,
                            "component": "layout/routerView/parent",
                            "title": "MCP 冒烟",
                            "icon": "i-mdi-test-tube",
                            "sort": 9900,
                            "enabled": true,
                            "children": [
                                {
                                    "name": child_name,
                                    "path": child_path,
                                    "component": "system/mcp-smoke/index",
                                    "title": "运行时配置",
                                    "sort": 1,
                                    "enabled": true,
                                    "buttons": [
                                        {
                                            "authName": "查看",
                                            "authMark": auth_mark,
                                            "sort": 1,
                                            "enabled": true
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            }))),
        )
        .await?;
    let menu_export_json = menu_export_result
        .structured_content
        .expect("expected structured content from menu_tool export_config");
    assert_eq!(menu_export_json["result"]["kind"], json!("config_export"));
    let menu_spec_file = menu_export_json["result"]["export"]["spec_file"]
        .as_str()
        .expect("expected menu export specFile");
    let menu_plan_file = menu_export_json["result"]["export"]["plan_file"]
        .as_str()
        .expect("expected menu export planFile");
    let menu_exported_spec = tokio::fs::read_to_string(menu_spec_file).await?;
    let menu_exported_plan = tokio::fs::read_to_string(menu_plan_file).await?;
    assert!(menu_exported_spec.contains(root_path));
    assert!(menu_exported_plan.contains("\"createCount\": 0"));

    let menu_list_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "list_tree"
            }))),
        )
        .await?;
    let menu_list_json = menu_list_result
        .structured_content
        .expect("expected structured content from menu_tool list_tree");
    let menu_items = menu_list_json["result"]["items"]
        .as_array()
        .expect("expected menu items array");
    assert!(contains_menu_path(menu_items, root_path));
    assert!(contains_menu_path(menu_items, child_path));
    assert!(contains_button_mark(menu_items, auth_mark));

    let _ = tokio::fs::remove_dir_all(&export_output_dir).await;

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SUMMER_MCP_DATABASE_URL and a local PostgreSQL instance"]
async fn standalone_binary_generates_showcase_bundle_with_art_design_pro_layout()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let transport = smoke_transport(smoke_database_url())?;
    let client = DummyClient::default().serve(transport).await?;
    let test_entity_file = prepare_showcase_profile_test_entity()?;

    let admin_output_dir = std::env::temp_dir().join("summer-mcp-showcase-admin-real");
    let frontend_output_dir = std::env::temp_dir().join("summer-mcp-showcase-art-design-pro");
    let _ = tokio::fs::remove_dir_all(&admin_output_dir).await;
    let _ = tokio::fs::remove_dir_all(&frontend_output_dir).await;

    client
        .call_tool(
            CallToolRequestParams::new("sql_exec").with_arguments(json_args(json!({
                "sql": create_showcase_profile_test_table_sql()
            }))),
        )
        .await?;

    let admin_result = client
        .call_tool(
            CallToolRequestParams::new("generate_admin_module_from_table").with_arguments(
                json_args(json!({
                    "table": TEST_SHOWCASE_PROFILE_TABLE,
                    "route_base": TEST_SHOWCASE_PROFILE_ROUTE_BASE,
                    "overwrite": true,
                    "output_dir": admin_output_dir.display().to_string()
                })),
            ),
        )
        .await?;
    let admin_json = admin_result
        .structured_content
        .expect("expected structured content from generate_admin_module_from_table");
    assert_eq!(admin_json["route_base"], json!("showcase_profile"));

    let generate_result = client
        .call_tool(
            CallToolRequestParams::new("generate_frontend_bundle_from_table").with_arguments(
                json_args(json!({
                    "table": TEST_SHOWCASE_PROFILE_TABLE,
                    "route_base": TEST_SHOWCASE_PROFILE_ROUTE_BASE,
                    "overwrite": true,
                    "output_dir": frontend_output_dir.display().to_string(),
                    "target_preset": "art_design_pro",
                    "dict_bindings": {
                        "status": "showcase_status",
                        "contact_gender": "showcase_gender"
                    },
                    "field_hints": {
                        "contact_gender": {
                            "search_widget": "radio_group"
                        }
                    },
                    "search_fields": [
                        "showcase_code",
                        "title",
                        "status",
                        "contact_gender",
                        "featured",
                        "publish_date",
                        "launch_at"
                    ],
                    "table_fields": [
                        "showcase_code",
                        "title",
                        "avatar",
                        "cover_image",
                        "contact_name",
                        "contact_gender",
                        "status",
                        "featured",
                        "priority",
                        "score",
                        "publish_date",
                        "launch_at",
                        "service_time",
                        "attachment_url",
                        "contact_email",
                        "official_url",
                        "created_at"
                    ],
                    "form_fields": [
                        "showcase_code",
                        "title",
                        "avatar",
                        "cover_image",
                        "contact_name",
                        "contact_gender",
                        "contact_phone",
                        "contact_email",
                        "official_url",
                        "status",
                        "featured",
                        "priority",
                        "score",
                        "publish_date",
                        "launch_at",
                        "service_time",
                        "attachment_url",
                        "description",
                        "extra_notes"
                    ]
                })),
            ),
        )
        .await?;

    let generate_json = generate_result
        .structured_content
        .expect("expected structured content from generate_frontend_bundle_from_table");
    assert_eq!(generate_json["table"], json!(TEST_SHOWCASE_PROFILE_TABLE));
    assert_eq!(generate_json["route_base"], json!("showcase_profile"));
    assert_eq!(
        generate_json["api_import_path"],
        json!("@/api/showcase-profile")
    );
    assert_eq!(generate_json["api_namespace"], json!("ShowcaseProfile"));
    assert_eq!(
        generate_json["required_dict_types"],
        json!(["showcase_gender", "showcase_status"])
    );

    let api_file = frontend_output_dir.join("src/api/showcase-profile.ts");
    let api_type_file = frontend_output_dir.join("src/types/api/showcase-profile.d.ts");
    let page_dir = frontend_output_dir.join("src/views/system/showcase-profile");
    let index_file = page_dir.join("index.vue");
    let search_file = page_dir.join("modules/showcase-profile-search.vue");
    let dialog_file = page_dir.join("modules/showcase-profile-dialog.vue");

    let api_contents = tokio::fs::read_to_string(&api_file).await?;
    let api_type_contents = tokio::fs::read_to_string(&api_type_file).await?;
    let index_contents = tokio::fs::read_to_string(&index_file).await?;
    let search_contents = tokio::fs::read_to_string(&search_file).await?;
    let dialog_contents = tokio::fs::read_to_string(&dialog_file).await?;

    assert!(api_contents.contains("fetchGetShowcaseProfileList"));
    assert!(api_contents.contains("fetchGetShowcaseProfileDetail"));
    assert!(api_type_contents.contains("namespace ShowcaseProfile"));
    assert!(api_type_contents.contains("interface ShowcaseProfileVo"));
    assert!(api_type_contents.contains("contactGender?: number"));
    assert!(api_type_contents.contains("status?: number"));

    assert!(index_contents.contains("from '@/api/showcase-profile'"));
    assert!(index_contents.contains("getDictLabel('showcase_status'"));
    assert!(index_contents.contains("getDictLabel('showcase_gender'"));
    assert!(index_contents.contains("ElImage"));
    assert!(index_contents.contains("mailto:"));
    assert!(index_contents.contains("row.officialUrl"));

    assert!(search_contents.contains("key: 'contactGender'"));
    assert!(search_contents.contains("type: 'radiogroup'"));
    assert!(search_contents.contains("getDict('showcase_gender')"));
    assert!(search_contents.contains("type: 'daterange'"));
    assert!(search_contents.contains("type: 'datetimerange'"));
    assert!(search_contents.contains("key: 'publishDateRange'"));
    assert!(search_contents.contains("key: 'launchAtRange'"));

    assert!(dialog_contents.contains("fetchCreateShowcaseProfile"));
    assert!(dialog_contents.contains("fetchGetShowcaseProfileDetail"));
    assert!(dialog_contents.contains("fetchUpdateShowcaseProfile"));
    assert!(dialog_contents.contains("ArtFileUpload"));
    assert!(dialog_contents.contains("handleAvatarUploadSuccess"));
    assert!(dialog_contents.contains("handleAttachmentUrlUploadSuccess"));
    assert!(dialog_contents.contains("type=\"textarea\""));
    assert!(dialog_contents.contains("getDict('showcase_status')"));
    assert!(dialog_contents.contains("getDict('showcase_gender')"));
    assert!(!dialog_contents.contains("createdAt"));
    assert!(!dialog_contents.contains("updatedAt"));

    let showcase_status_bundle = json!({
        "dictName": "展示状态",
        "dictType": "showcase_status",
        "items": [
            {
                "dictLabel": "草稿",
                "dictValue": "1",
                "dictSort": 0,
                "listClass": "info",
                "isDefault": true
            },
            {
                "dictLabel": "已发布",
                "dictValue": "2",
                "dictSort": 1,
                "listClass": "success"
            },
            {
                "dictLabel": "已归档",
                "dictValue": "3",
                "dictSort": 2,
                "listClass": "warning"
            }
        ]
    });
    let showcase_gender_bundle = json!({
        "dictName": "联系人性别",
        "dictType": "showcase_gender",
        "items": [
            {
                "dictLabel": "未知",
                "dictValue": "0",
                "dictSort": 0
            },
            {
                "dictLabel": "男",
                "dictValue": "1",
                "dictSort": 1
            },
            {
                "dictLabel": "女",
                "dictValue": "2",
                "dictSort": 2
            }
        ]
    });

    for bundle in [showcase_status_bundle, showcase_gender_bundle] {
        let dict_plan_result = client
            .call_tool(
                CallToolRequestParams::new("dict_tool").with_arguments(json_args(json!({
                    "action": "plan_bundle",
                    "bundle": bundle
                }))),
            )
            .await?;
        let dict_plan_json = dict_plan_result
            .structured_content
            .expect("expected structured content from dict_tool plan_bundle");
        assert_eq!(dict_plan_json["result"]["kind"], json!("bundle_sync"));
    }

    let menu_plan_result = client
        .call_tool(
            CallToolRequestParams::new("menu_tool").with_arguments(json_args(json!({
                "action": "plan_config",
                "config": generate_json["menu_config_draft"].clone()
            }))),
        )
        .await?;
    let menu_plan_json = menu_plan_result
        .structured_content
        .expect("expected structured content from menu_tool plan_config");
    assert_eq!(menu_plan_json["result"]["kind"], json!("config_sync"));

    let _ = client
        .call_tool(
            CallToolRequestParams::new("sql_exec").with_arguments(json_args(json!({
                "sql": drop_showcase_profile_test_table_sql()
            }))),
        )
        .await;
    let _ = tokio::fs::remove_file(&test_entity_file).await;

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SUMMER_MCP_DATABASE_URL, SUMMER_MCP_ART_DESIGN_PRO_DIR, pnpm, and a local PostgreSQL instance"]
async fn standalone_binary_typechecks_showcase_bundle_against_real_art_design_pro_project()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let transport = smoke_transport(smoke_database_url())?;
    let client = DummyClient::default().serve(transport).await?;
    let test_entity_file = prepare_showcase_profile_test_entity()?;

    let preview_root = std::env::temp_dir().join("summer-mcp-showcase-art-design-pro-preview");
    let _ = tokio::fs::remove_dir_all(&preview_root).await;

    let test_result = async {
        prepare_art_design_pro_preview(&smoke_art_design_pro_dir(), &preview_root).await?;

        client
            .call_tool(
                CallToolRequestParams::new("sql_exec").with_arguments(json_args(json!({
                    "sql": create_showcase_profile_test_table_sql()
                }))),
            )
            .await?;

        client
            .call_tool(
                CallToolRequestParams::new("generate_frontend_bundle_from_table").with_arguments(
                    json_args(json!({
                    "table": TEST_SHOWCASE_PROFILE_TABLE,
                    "route_base": TEST_SHOWCASE_PROFILE_ROUTE_BASE,
                    "overwrite": true,
                    "output_dir": preview_root.display().to_string(),
                    "target_preset": "art_design_pro",
                    "dict_bindings": {
                        "status": "showcase_status",
                        "contact_gender": "showcase_gender"
                        },
                        "field_hints": {
                            "contact_gender": {
                                "search_widget": "radio_group"
                            }
                        },
                        "search_fields": [
                            "showcase_code",
                            "title",
                            "status",
                            "contact_gender",
                            "featured",
                            "publish_date",
                            "launch_at"
                        ],
                        "table_fields": [
                            "showcase_code",
                            "title",
                            "avatar",
                            "cover_image",
                            "contact_name",
                            "contact_gender",
                            "status",
                            "featured",
                            "priority",
                            "score",
                            "publish_date",
                            "launch_at",
                            "service_time",
                            "attachment_url",
                            "contact_email",
                            "official_url",
                            "created_at"
                        ],
                        "form_fields": [
                            "showcase_code",
                            "title",
                            "avatar",
                            "cover_image",
                            "contact_name",
                            "contact_gender",
                            "contact_phone",
                            "contact_email",
                            "official_url",
                            "status",
                            "featured",
                            "priority",
                            "score",
                            "publish_date",
                            "launch_at",
                            "service_time",
                            "attachment_url",
                            "description",
                            "extra_notes"
                        ]
                    })),
                ),
            )
            .await?;

        let mut typecheck = Command::new("pnpm");
        typecheck
            .arg("exec")
            .arg("vue-tsc")
            .arg("--noEmit")
            .arg("-p")
            .arg("tsconfig.generated-showcase.json")
            .current_dir(&preview_root);
        assert_command_success(typecheck, "generated showcase bundle vue-tsc").await?;

        let mut lint = Command::new("pnpm");
        lint.arg("exec")
            .arg("eslint")
            .arg("src/views/system/showcase-profile")
            .arg("src/api/showcase-profile.ts")
            .arg("src/types/api/showcase-profile.d.ts")
            .current_dir(&preview_root);
        assert_command_success(lint, "generated showcase bundle eslint").await?;

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    let _ = client
        .call_tool(
            CallToolRequestParams::new("sql_exec").with_arguments(json_args(json!({
                "sql": drop_showcase_profile_test_table_sql()
            }))),
        )
        .await;
    let _ = tokio::fs::remove_file(&test_entity_file).await;
    let _ = tokio::fs::remove_dir_all(&preview_root).await;

    client.cancel().await?;
    test_result
}
