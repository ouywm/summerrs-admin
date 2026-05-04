use summer_job_dynamic::{HandlerRegistry, JobContext, JobResult, job_handler};

#[job_handler("__macro_smoke_handler__")]
async fn smoke(_ctx: JobContext) -> JobResult {
    Ok(serde_json::json!({"ok": true}))
}

#[test]
fn macro_registers_to_inventory() {
    let registry = HandlerRegistry::collect();
    assert!(
        registry.contains("__macro_smoke_handler__"),
        "expected handler registered, got: {:?}",
        registry.names()
    );
}
