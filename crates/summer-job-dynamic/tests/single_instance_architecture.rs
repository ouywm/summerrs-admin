#[test]
fn plugin_no_longer_uses_redis_event_bus_or_leader_election() {
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
fn job_service_uses_scheduler_handle_instead_of_event_bus() {
    let service = include_str!("../src/service/job_service.rs");

    assert!(
        !service.contains("EventBus"),
        "JobService should sync the local scheduler directly, not publish events"
    );
    assert!(
        !service.contains("SchedulerEvent"),
        "JobService should not build scheduler events in single-instance mode"
    );
    assert!(
        service.contains("SchedulerHandle"),
        "JobService should use SchedulerHandle for local runtime synchronization"
    );
}
