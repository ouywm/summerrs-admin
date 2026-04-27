use summer_ai_model::dto::user_quota::{
    AdjustUserQuotaDto, CreateUserQuotaDto, UpdateUserQuotaDto,
};
use summer_ai_model::entity::billing::user_quota::{self, UserQuotaStatus};
use summer_ai_model::vo::user_quota::UserQuotaVo;

#[test]
fn create_user_quota_defaults_runtime_counters() {
    let dto = CreateUserQuotaDto {
        user_id: 42,
        channel_group: Some("vip".into()),
        status: None,
        quota: 10_000,
        daily_quota_limit: Some(1_000),
        monthly_quota_limit: Some(20_000),
        remark: Some("initial".into()),
    };

    let active = dto.into_active_model("operator");

    assert_eq!(active.user_id.unwrap(), 42);
    assert_eq!(active.channel_group.unwrap(), "vip");
    assert_eq!(active.status.unwrap(), UserQuotaStatus::Normal);
    assert_eq!(active.quota.unwrap(), 10_000);
    assert_eq!(active.used_quota.unwrap(), 0);
    assert_eq!(active.request_count.unwrap(), 0);
    assert_eq!(active.daily_used_quota.unwrap(), 0);
    assert_eq!(active.monthly_used_quota.unwrap(), 0);
}

#[test]
fn update_user_quota_does_not_reset_usage_counters() {
    let now = chrono::Utc::now().fixed_offset();
    let model = user_quota::Model {
        id: 1,
        user_id: 42,
        channel_group: "default".into(),
        status: UserQuotaStatus::Normal,
        quota: 10_000,
        used_quota: 7_000,
        request_count: 9,
        daily_quota_limit: 1000,
        monthly_quota_limit: 20_000,
        daily_used_quota: 300,
        monthly_used_quota: 700,
        daily_window_start: None,
        monthly_window_start: None,
        last_request_time: None,
        remark: "old".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: user_quota::ActiveModel = model.into();

    UpdateUserQuotaDto {
        channel_group: Some("vip".into()),
        status: Some(UserQuotaStatus::Frozen),
        quota: Some(12_000),
        daily_quota_limit: Some(2_000),
        monthly_quota_limit: None,
        remark: Some("updated".into()),
    }
    .apply_to(&mut active, "operator");

    assert_eq!(active.channel_group.unwrap(), "vip");
    assert_eq!(active.status.unwrap(), UserQuotaStatus::Frozen);
    assert_eq!(active.quota.unwrap(), 12_000);
    assert_eq!(active.used_quota.unwrap(), 7_000);
    assert_eq!(active.request_count.unwrap(), 9);
    assert_eq!(active.daily_used_quota.unwrap(), 300);
    assert_eq!(active.monthly_used_quota.unwrap(), 700);
}

#[test]
fn user_quota_vo_exposes_remaining_quota() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = UserQuotaVo::from_model(user_quota::Model {
        id: 1,
        user_id: 42,
        channel_group: "vip".into(),
        status: UserQuotaStatus::Normal,
        quota: 10_000,
        used_quota: 7_500,
        request_count: 9,
        daily_quota_limit: 1000,
        monthly_quota_limit: 20_000,
        daily_used_quota: 300,
        monthly_used_quota: 700,
        daily_window_start: None,
        monthly_window_start: None,
        last_request_time: None,
        remark: "remark".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.remaining_quota, 2_500);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}

#[test]
fn adjust_user_quota_rejects_zero_delta() {
    let dto = AdjustUserQuotaDto {
        quota_delta: 0,
        reference_no: None,
        reason: Some("noop".into()),
    };

    assert!(dto.validate_business_rules().is_err());
}
