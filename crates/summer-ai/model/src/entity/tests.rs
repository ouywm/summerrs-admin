use std::fs;
use std::path::PathBuf;

fn entity_source_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/entity");
    let mut stack = vec![dir];
    let mut files = Vec::new();

    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path).unwrap_or_else(|_| panic!("read {}", path.display())) {
            let entry = entry.unwrap_or_else(|_| panic!("read {}", path.display()));
            let entry_path = entry.path();

            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }

            if entry_path.extension().and_then(|ext| ext.to_str()) == Some("rs")
                && !matches!(
                    entry_path.file_name().and_then(|name| name.to_str()),
                    Some("mod.rs" | "tests.rs")
                )
            {
                files.push(entry_path);
            }
        }
    }

    files.sort();
    files
}

fn entity_source_file(table: &str) -> PathBuf {
    entity_source_files()
        .into_iter()
        .find(|path| path.file_stem().and_then(|stem| stem.to_str()) == Some(table))
        .unwrap_or_else(|| panic!("missing entity source file for table {table}"))
}

fn sql_source_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../sql/ai")
        .canonicalize()
        .expect("canonicalize sql/ai");
    let mut files = fs::read_dir(dir)
        .expect("read sql/ai dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .filter(|path| path.file_name().and_then(|name| name.to_str()) != Some("README.md"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn sql_columns(path: &PathBuf) -> Vec<String> {
    let source = fs::read_to_string(path).unwrap_or_else(|_| panic!("read {}", path.display()));
    let mut in_table = false;
    let mut columns = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("CREATE TABLE ") {
            in_table = true;
            continue;
        }

        if !in_table {
            continue;
        }

        if trimmed == ");" {
            break;
        }

        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }

        if matches!(
            trimmed.split_whitespace().next(),
            Some("PRIMARY" | "UNIQUE" | "CONSTRAINT" | "CHECK" | "FOREIGN")
        ) {
            continue;
        }

        if let Some((column, _rest)) = trimmed.split_once(char::is_whitespace) {
            columns.push(column.trim_matches('"').to_string());
        }
    }

    columns
}

fn rust_model_fields(path: &PathBuf) -> Vec<String> {
    let source = fs::read_to_string(path).unwrap_or_else(|_| panic!("read {}", path.display()));
    let mut fields = Vec::new();
    let mut in_model = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("pub struct Model") && trimmed.ends_with('{') {
            in_model = true;
            continue;
        }

        if !in_model {
            continue;
        }

        if trimmed == "}" {
            break;
        }

        if !trimmed.starts_with("pub ") || !trimmed.contains(':') {
            continue;
        }

        let field = trimmed
            .strip_prefix("pub ")
            .and_then(|rest| rest.split_once(':'))
            .map(|(name, ty)| (name.trim(), ty.trim().trim_end_matches(',')))
            .expect("parse rust model field");

        if field.1.contains("HasMany<") || field.1.contains("super::") {
            continue;
        }

        fields.push(field.0.to_string());
    }

    fields
}

#[test]
fn entity_fields_and_numeric_enums_have_doc_comments() {
    let mut failures = Vec::new();

    for path in entity_source_files() {
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
        let lines = source.lines().collect::<Vec<_>>();
        let mut in_model = false;
        let mut in_attribute = false;
        let mut seen_doc = false;
        let mut pending_num_value = false;

        for line in lines {
            let trimmed = line.trim();

            if trimmed.starts_with("///") {
                seen_doc = true;
                continue;
            }

            if in_attribute {
                if trimmed.ends_with(']') {
                    in_attribute = false;
                }
                continue;
            }

            if trimmed.starts_with("#[sea_orm(num_value") {
                if !seen_doc {
                    failures.push(format!(
                        "{}: numeric enum variant attribute lacks preceding doc comment",
                        path.display()
                    ));
                }
                pending_num_value = true;
                continue;
            }

            if trimmed.starts_with("#[") {
                if !trimmed.ends_with(']') {
                    in_attribute = true;
                }
                continue;
            }

            if trimmed.starts_with("pub enum ") && trimmed.ends_with('{') {
                if !seen_doc {
                    failures.push(format!(
                        "{}: enum declaration lacks preceding doc comment: {}",
                        path.display(),
                        trimmed
                    ));
                }
                seen_doc = false;
                continue;
            }

            if trimmed.starts_with("pub struct Model") && trimmed.ends_with('{') {
                in_model = true;
                seen_doc = false;
                continue;
            }

            if in_model && trimmed == "}" {
                in_model = false;
                seen_doc = false;
                continue;
            }

            if in_model && trimmed.starts_with("pub ") && trimmed.contains(':') {
                if !seen_doc {
                    failures.push(format!(
                        "{}: field lacks preceding doc comment: {}",
                        path.display(),
                        trimmed
                    ));
                }
                seen_doc = false;
                continue;
            }

            if pending_num_value && trimmed.contains('=') && trimmed.ends_with(',') {
                pending_num_value = false;
                seen_doc = false;
                continue;
            }

            if !trimmed.is_empty() {
                seen_doc = false;
            }
        }
    }

    assert!(
        failures.is_empty(),
        "missing entity docs:\n{}",
        failures.join("\n")
    );
}

#[test]
fn selected_relations_live_in_grouped_entity_modules() {
    let entity_expectations = [
        (
            "ability",
            "pub channel: Option<super::channel::Entity>",
            true,
        ),
        (
            "alert_event",
            "pub alert_rule: Option<super::alert_rule::Entity>",
            true,
        ),
        (
            "alert_rule",
            "pub events: HasMany<super::alert_event::Entity>",
            false,
        ),
        (
            "alert_rule",
            "pub silences: HasMany<super::alert_silence::Entity>",
            false,
        ),
        (
            "alert_silence",
            "pub alert_rule: Option<super::alert_rule::Entity>",
            true,
        ),
        (
            "channel",
            "pub channel_accounts: HasMany<super::channel_account::Entity>",
            false,
        ),
        (
            "channel",
            "pub abilities: HasMany<super::ability::Entity>",
            false,
        ),
        (
            "channel_account",
            "pub channel: Option<super::channel::Entity>",
            true,
        ),
        (
            "conversation",
            "pub message_entities: HasMany<super::message::Entity>",
            false,
        ),
        (
            "guardrail_config",
            "pub rules: HasMany<super::guardrail_rule::Entity>",
            false,
        ),
        (
            "guardrail_rule",
            "pub guardrail_config: Option<super::guardrail_config::Entity>",
            true,
        ),
        (
            "message",
            "pub conversation: Option<super::conversation::Entity>",
            true,
        ),
        (
            "request",
            "pub executions: HasMany<super::request_execution::Entity>",
            false,
        ),
        (
            "request_execution",
            "pub request: Option<super::request::Entity>",
            true,
        ),
        (
            "trace",
            "pub spans: HasMany<super::trace_span::Entity>",
            false,
        ),
        (
            "trace_span",
            "pub trace: Option<super::trace::Entity>",
            true,
        ),
    ];

    let mut failures = Vec::new();

    for (file, snippet, requires_skip_fk) in entity_expectations {
        let path = entity_source_file(file);
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
        if !source.contains(snippet) {
            failures.push(format!(
                "{}: expected entity model relation snippet missing: {}",
                path.display(),
                snippet
            ));
        }
        if requires_skip_fk && !source.contains("skip_fk") {
            failures.push(format!(
                "{}: belongs_to logical relations should use skip_fk",
                path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "entity relation ownership issues:\n{}",
        failures.join("\n")
    );
}

#[test]
fn grouped_entity_modules_are_exposed_without_breaking_top_level_reexports() {
    let _ = std::any::TypeId::of::<super::alerts::alert_rule::Entity>();
    let _ = std::any::TypeId::of::<super::channels::channel::Entity>();
    let _ = std::any::TypeId::of::<super::channels::channel_account::Entity>();
    let _ = std::any::TypeId::of::<super::conversations::conversation::Entity>();
    let _ = std::any::TypeId::of::<super::file_storage::vector_store::Entity>();
    let _ = std::any::TypeId::of::<super::guardrails::guardrail_config::Entity>();
    let _ = std::any::TypeId::of::<super::tenancy::organization::Entity>();
    let _ = std::any::TypeId::of::<super::billing::transaction::Entity>();
    let _ = std::any::TypeId::of::<super::requests::request::Entity>();
    let _ = std::any::TypeId::of::<super::channel::Entity>();
    let _ = std::any::TypeId::of::<super::organization::Entity>();
}

#[test]
fn legacy_entity_container_is_removed() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/entity/_entity");
    assert!(
        !path.exists(),
        "legacy _entity directory should be removed, found {}",
        path.display()
    );
}

#[test]
fn vector_store_entity_declares_docs_and_status_enum() {
    let path = entity_source_file("vector_store");
    let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));

    assert!(
        source.contains("/// 向量库状态"),
        "missing vector_store status enum docs"
    );
    assert!(
        source.contains("pub enum VectorStoreStatus"),
        "missing VectorStoreStatus enum"
    );
    assert!(
        source.contains("pub status: VectorStoreStatus"),
        "vector_store status field should use VectorStoreStatus"
    );
    assert!(
        source.contains("/// 向量库ID"),
        "missing vector_store field docs"
    );
    assert!(
        source.contains("/// 上游向量库ID"),
        "missing vector_store SQL-aligned docs"
    );
}

#[test]
fn vector_store_file_entity_declares_docs_and_status_enum() {
    let path = entity_source_file("vector_store_file");
    let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));

    assert!(
        source.contains("/// 向量库文件状态"),
        "missing vector_store_file status enum docs"
    );
    assert!(
        source.contains("pub enum VectorStoreFileStatus"),
        "missing VectorStoreFileStatus enum"
    );
    assert!(
        source.contains("pub status: VectorStoreFileStatus"),
        "vector_store_file status field should use VectorStoreFileStatus"
    );
    assert!(
        source.contains("/// 关联ID"),
        "missing vector_store_file field docs"
    );
    assert!(
        source.contains("/// 最近错误信息（JSON）"),
        "missing vector_store_file SQL-aligned docs"
    );
}

#[test]
fn sql_tables_have_corresponding_entity_files() {
    let sql_tables = sql_source_files()
        .into_iter()
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let entity_tables = entity_source_files()
        .into_iter()
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        entity_tables, sql_tables,
        "entity files should match sql/ai tables exactly"
    );
}

#[test]
fn entity_model_fields_follow_sql_column_order() {
    let sql_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../sql/ai")
        .canonicalize()
        .expect("canonicalize sql/ai");
    let mut failures = Vec::new();

    for path in entity_source_files() {
        let table = path.file_stem().unwrap().to_string_lossy().into_owned();
        let sql_path = sql_dir.join(format!("{table}.sql"));

        if !sql_path.exists() {
            failures.push(format!(
                "{}: missing matching SQL file {}",
                path.display(),
                sql_path.display()
            ));
            continue;
        }

        let sql_fields = sql_columns(&sql_path);
        let rust_fields = rust_model_fields(&path);

        if sql_fields != rust_fields {
            failures.push(format!(
                "{}: field order drift\n  sql : {:?}\n  rust: {:?}",
                path.display(),
                sql_fields,
                rust_fields
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "entity fields should follow sql columns:\n{}",
        failures.join("\n")
    );
}

#[test]
fn entities_with_update_time_use_before_save_hooks() {
    let mut failures = Vec::new();

    for path in entity_source_files() {
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));

        if !source.contains("pub update_time:") {
            continue;
        }

        if !source.contains("before_save<C>") {
            failures.push(format!(
                "{}: missing before_save hook for update_time maintenance",
                path.display()
            ));
        }

        if !source.contains("self.update_time = sea_orm::Set(now);") {
            failures.push(format!(
                "{}: before_save hook should maintain update_time",
                path.display()
            ));
        }

        if source.contains("pub create_time:")
            && !source.contains("self.create_time = sea_orm::Set(now);")
        {
            failures.push(format!(
                "{}: before_save hook should initialize create_time on insert",
                path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "timestamp hook issues:\n{}",
        failures.join("\n")
    );
}

#[test]
fn entities_with_create_time_only_use_before_save_hooks() {
    let mut failures = Vec::new();

    for path in entity_source_files() {
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));

        if !source.contains("pub create_time:") || source.contains("pub update_time:") {
            continue;
        }

        if !source.contains("before_save<C>") {
            failures.push(format!(
                "{}: missing before_save hook for create_time maintenance",
                path.display()
            ));
        }

        if !source.contains("self.create_time = sea_orm::Set(now);") {
            failures.push(format!(
                "{}: before_save hook should initialize create_time on insert",
                path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "create_time hook issues:\n{}",
        failures.join("\n")
    );
}
