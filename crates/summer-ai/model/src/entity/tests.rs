use std::fs;
use std::path::PathBuf;

fn entity_source_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/entity/_entity");
    let mut files = fs::read_dir(dir)
        .expect("read _entity dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .collect::<Vec<_>>();
    files.sort();
    files
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
fn selected_relations_live_in_entity_models_not_wrapper_modules() {
    let wrapper_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/entity");
    let entity_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/entity/_entity");

    let wrapper_expectations = [
        "ability.rs",
        "alert_event.rs",
        "alert_rule.rs",
        "alert_silence.rs",
        "channel.rs",
        "channel_account.rs",
        "conversation.rs",
        "guardrail_config.rs",
        "guardrail_rule.rs",
        "message.rs",
        "request.rs",
        "request_execution.rs",
        "trace.rs",
        "trace_span.rs",
    ];
    let entity_expectations = [
        (
            "ability.rs",
            "pub channel: Option<super::channel::Entity>",
            true,
        ),
        (
            "alert_event.rs",
            "pub alert_rule: Option<super::alert_rule::Entity>",
            true,
        ),
        (
            "alert_rule.rs",
            "pub events: HasMany<super::alert_event::Entity>",
            false,
        ),
        (
            "alert_rule.rs",
            "pub silences: HasMany<super::alert_silence::Entity>",
            false,
        ),
        (
            "alert_silence.rs",
            "pub alert_rule: Option<super::alert_rule::Entity>",
            true,
        ),
        (
            "channel.rs",
            "pub channel_accounts: HasMany<super::channel_account::Entity>",
            false,
        ),
        (
            "channel.rs",
            "pub abilities: HasMany<super::ability::Entity>",
            false,
        ),
        (
            "channel_account.rs",
            "pub channel: Option<super::channel::Entity>",
            true,
        ),
        (
            "conversation.rs",
            "pub message_entities: HasMany<super::message::Entity>",
            false,
        ),
        (
            "guardrail_config.rs",
            "pub rules: HasMany<super::guardrail_rule::Entity>",
            false,
        ),
        (
            "guardrail_rule.rs",
            "pub guardrail_config: Option<super::guardrail_config::Entity>",
            true,
        ),
        (
            "message.rs",
            "pub conversation: Option<super::conversation::Entity>",
            true,
        ),
        (
            "request.rs",
            "pub executions: HasMany<super::request_execution::Entity>",
            false,
        ),
        (
            "request_execution.rs",
            "pub request: Option<super::request::Entity>",
            true,
        ),
        (
            "trace.rs",
            "pub spans: HasMany<super::trace_span::Entity>",
            false,
        ),
        (
            "trace_span.rs",
            "pub trace: Option<super::trace::Entity>",
            true,
        ),
    ];

    let mut failures = Vec::new();

    for file in wrapper_expectations {
        let path = wrapper_dir.join(file);
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
        if source.contains("impl Related<") {
            failures.push(format!(
                "{}: wrapper module should not define impl Related",
                path.display()
            ));
        }
        if source.contains("belongs_to =") || source.contains("has_many =") {
            failures.push(format!(
                "{}: wrapper module should not define relation attributes",
                path.display()
            ));
        }
    }

    for (file, snippet, requires_skip_fk) in entity_expectations {
        let path = entity_dir.join(file);
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
