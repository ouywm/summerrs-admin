//! 单机调度器架构守护测试：确保重构后的 plugin / service 没引入多机依赖。

#[test]
fn plugin_stays_single_instance() {
    let plugin = include_str!("../src/plugin.rs");

    assert!(
        !plugin.contains("EventBus"),
        "single-instance scheduler must not use EventBus"
    );
    assert!(
        !plugin.contains("SchedulerEvent"),
        "single-instance scheduler must not route through SchedulerEvent"
    );
    assert!(
        !plugin.contains("LeaderElector"),
        "single-instance scheduler must not perform leader election"
    );
    assert!(
        !plugin.contains("RedisPlugin"),
        "single-instance scheduler must not depend on RedisPlugin"
    );
}

#[test]
fn job_service_uses_scheduler_handle_directly() {
    let service = include_str!("../src/service/job_service.rs");

    assert!(
        !service.contains("EventBus"),
        "JobService should sync the local scheduler directly, not publish events"
    );
    assert!(
        service.contains("SchedulerHandle"),
        "JobService should use SchedulerHandle for local runtime synchronization"
    );
}
