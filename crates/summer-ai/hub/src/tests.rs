use chrono::{FixedOffset, TimeZone};
use std::fs;
use std::path::PathBuf;

#[test]
fn ddd_modules_are_exposed() {
    let _ = crate::application::ApplicationModule;
    let _ = crate::domain::DomainModule;
    let _ = crate::infrastructure::InfrastructureModule;
    let _ = crate::interfaces::InterfaceModule;
    let _ = crate::interfaces::http::HttpInterfaceModule;
    let _ = crate::plugin::SummerAiHubPlugin;
}

#[derive(Clone)]
struct FakeGuardrailConfigRepository {
    item: Option<crate::domain::guardrail_config::GuardrailConfigAggregate>,
}

#[summer::async_trait]
impl crate::domain::guardrail_config::GuardrailConfigReadRepository
    for FakeGuardrailConfigRepository
{
    async fn find_by_id(
        &self,
        _id: i64,
    ) -> crate::domain::guardrail_config::DomainResult<
        Option<crate::domain::guardrail_config::GuardrailConfigAggregate>,
    > {
        Ok(self.item.clone())
    }
}

#[tokio::test]
async fn guardrail_config_application_service_returns_detail() {
    let repository = FakeGuardrailConfigRepository {
        item: Some(crate::domain::guardrail_config::GuardrailConfigAggregate {
            id: 7,
            scope_type: "organization".to_string(),
            organization_id: 100,
            project_id: 200,
            enabled: true,
            mode: "enforce".to_string(),
            system_rules: serde_json::json!(["pii", "secret"]),
            allowed_file_types: serde_json::json!(["pdf"]),
            max_file_size_mb: 32,
            pii_action: "redact".to_string(),
            secret_action: "block".to_string(),
            metadata: serde_json::json!({ "sample": true }),
            remark: "ddd sample".to_string(),
            create_time: FixedOffset::east_opt(8 * 3600)
                .expect("offset")
                .with_ymd_and_hms(2026, 4, 5, 10, 0, 0)
                .single()
                .expect("create time"),
            update_time: FixedOffset::east_opt(8 * 3600)
                .expect("offset")
                .with_ymd_and_hms(2026, 4, 5, 12, 0, 0)
                .single()
                .expect("update time"),
        }),
    };

    let use_case =
        crate::application::guardrail_config::GetGuardrailConfigDetailUseCase::new(repository);

    let detail = use_case
        .execute(crate::application::guardrail_config::GetGuardrailConfigDetailQuery { id: 7 })
        .await
        .expect("guardrail config detail");

    assert_eq!(detail.id, 7);
    assert_eq!(detail.scope_type, "organization");
    assert_eq!(detail.organization_id, 100);
    assert_eq!(detail.project_id, 200);
    assert_eq!(detail.mode, "enforce");
    assert_eq!(detail.max_file_size_mb, 32);
    assert_eq!(detail.remark, "ddd sample");
}

#[test]
fn guardrail_config_http_router_builds() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/interfaces/http");
    let mod_source = fs::read_to_string(base.join("mod.rs")).expect("read http mod");
    let handler_source =
        fs::read_to_string(base.join("guardrail_config.rs")).expect("read guardrail http handler");

    assert!(
        !mod_source.contains("pub fn router()"),
        "http mod should rely on macro auto registration instead of manual router builders"
    );
    assert!(
        !handler_source.contains("pub fn router()"),
        "http handler module should rely on macro auto registration instead of manual router builders"
    );
}
