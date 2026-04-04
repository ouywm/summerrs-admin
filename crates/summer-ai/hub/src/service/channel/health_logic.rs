use rand::RngExt;

use super::ChannelService;
use summer_ai_model::entity::channel::ChannelStatus;
use summer_ai_model::entity::channel_account::{self, AccountStatus};

const RATE_LIMIT_COOLDOWN_SECONDS: i64 = 60;
const OVERLOAD_COOLDOWN_SECONDS: i64 = 15;
const MAX_FAILURE_COOLDOWN_MULTIPLIER: i32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FailureCooldownWindow {
    pub(super) rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(super) overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SuccessHealthUpdate {
    pub(super) next_channel_status: ChannelStatus,
    pub(super) next_rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(super) next_overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(super) invalidate_route_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FailureHealthUpdate {
    pub(super) penalize: bool,
    pub(super) quarantine_account: bool,
    pub(super) next_channel_failure_streak: i32,
    pub(super) next_account_failure_streak: i32,
    pub(super) next_channel_status: ChannelStatus,
    pub(super) next_account_status: AccountStatus,
    pub(super) next_account_schedulable: bool,
    pub(super) next_health_status: i16,
    pub(super) cooldown: FailureCooldownWindow,
    pub(super) invalidate_route_cache: bool,
}

pub(super) fn failure_cooldown_window(
    status_code: i32,
    failure_streak: i32,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> FailureCooldownWindow {
    let multiplier = i64::from(failure_streak.clamp(1, MAX_FAILURE_COOLDOWN_MULTIPLIER));

    match status_code {
        429 => FailureCooldownWindow {
            rate_limited_until: Some(
                now + chrono::Duration::seconds(RATE_LIMIT_COOLDOWN_SECONDS * multiplier),
            ),
            overload_until: None,
        },
        0 | 408 | 409 | 425 | 500 | 502 | 503 | 504 => FailureCooldownWindow {
            rate_limited_until: None,
            overload_until: Some(
                now + chrono::Duration::seconds(OVERLOAD_COOLDOWN_SECONDS * multiplier),
            ),
        },
        _ => FailureCooldownWindow {
            rate_limited_until: None,
            overload_until: None,
        },
    }
}

pub(super) fn compute_test_success_health_update(
    current_channel_status: ChannelStatus,
    rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> SuccessHealthUpdate {
    let next_channel_status = if current_channel_status == ChannelStatus::AutoDisabled {
        ChannelStatus::Enabled
    } else {
        current_channel_status
    };
    let account_reentered = rate_limited_until.is_some_and(|recover_at| recover_at > now)
        || overload_until.is_some_and(|recover_at| recover_at > now);

    SuccessHealthUpdate {
        next_channel_status,
        next_rate_limited_until: None,
        next_overload_until: None,
        invalidate_route_cache: next_channel_status != current_channel_status || account_reentered,
    }
}

pub(super) fn compute_relay_success_health_update(
    current_channel_status: ChannelStatus,
    rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> SuccessHealthUpdate {
    SuccessHealthUpdate {
        next_channel_status: current_channel_status,
        next_rate_limited_until: rate_limited_until.filter(|recover_at| *recover_at > now),
        next_overload_until: overload_until.filter(|recover_at| *recover_at > now),
        invalidate_route_cache: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compute_failure_health_update(
    status_code: i32,
    current_channel_status: ChannelStatus,
    current_account_status: AccountStatus,
    current_account_schedulable: bool,
    channel_failure_streak: i32,
    account_failure_streak: i32,
    auto_ban: bool,
    existing_rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    existing_overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> FailureHealthUpdate {
    let penalize = should_penalize_upstream_health_failure(status_code);
    let quarantine_account = should_quarantine_account_on_auth_failure(status_code);
    let next_channel_failure_streak = if penalize {
        channel_failure_streak + 1
    } else {
        channel_failure_streak
    };
    let next_account_failure_streak = if penalize || quarantine_account {
        account_failure_streak + 1
    } else {
        account_failure_streak
    };
    let next_channel_status = if penalize && auto_ban && next_channel_failure_streak >= 3 {
        ChannelStatus::AutoDisabled
    } else {
        current_channel_status
    };
    let next_account_status = if quarantine_account {
        AccountStatus::Disabled
    } else {
        current_account_status
    };
    let next_account_schedulable = if quarantine_account {
        false
    } else {
        current_account_schedulable
    };
    let cooldown = if penalize {
        failure_cooldown_window(status_code, next_account_failure_streak, now)
    } else {
        FailureCooldownWindow {
            rate_limited_until: existing_rate_limited_until,
            overload_until: existing_overload_until,
        }
    };
    let account_availability_changed = existing_rate_limited_until != cooldown.rate_limited_until
        || existing_overload_until != cooldown.overload_until
        || next_account_status != current_account_status
        || next_account_schedulable != current_account_schedulable;

    FailureHealthUpdate {
        penalize,
        quarantine_account,
        next_channel_failure_streak,
        next_account_failure_streak,
        next_channel_status,
        next_account_status,
        next_account_schedulable,
        next_health_status: if penalize { 3 } else { 2 },
        cooldown,
        invalidate_route_cache: next_channel_status != current_channel_status
            || penalize
            || quarantine_account
            || account_availability_changed,
    }
}

pub(super) fn should_penalize_upstream_health_failure(status_code: i32) -> bool {
    matches!(status_code, 0 | 408 | 409 | 425 | 429 | 500..=599)
}

pub(super) fn should_quarantine_account_on_auth_failure(status_code: i32) -> bool {
    matches!(status_code, 401 | 403)
}

pub(super) fn relay_request_started_at(
    finished_at: chrono::DateTime<chrono::FixedOffset>,
    elapsed_ms: i64,
) -> chrono::DateTime<chrono::FixedOffset> {
    finished_at - chrono::Duration::milliseconds(elapsed_ms.max(0))
}

pub(super) fn relay_health_update_is_stale(
    channel_last_error_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    account_last_error_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    request_started_at: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    channel_last_error_at.is_some_and(|error_at| error_at > request_started_at)
        || account_last_error_at.is_some_and(|error_at| error_at > request_started_at)
}

pub(super) fn select_schedulable_account(
    accounts: Vec<channel_account::Model>,
) -> Option<channel_account::Model> {
    let now = chrono::Utc::now().fixed_offset();
    let candidates: Vec<channel_account::Model> = accounts
        .into_iter()
        .filter(|account| {
            account.expires_at.is_none_or(|expires_at| expires_at > now)
                && account
                    .rate_limited_until
                    .is_none_or(|recover_at| recover_at <= now)
                && account
                    .overload_until
                    .is_none_or(|recover_at| recover_at <= now)
                && !ChannelService::extract_api_key(&account.credentials).is_empty()
        })
        .collect();

    let max_priority = candidates.iter().map(|account| account.priority).max()?;
    let top_priority_accounts: Vec<channel_account::Model> = candidates
        .into_iter()
        .filter(|account| account.priority == max_priority)
        .collect();

    let positive_weight_accounts: Vec<channel_account::Model> = top_priority_accounts
        .iter()
        .filter(|account| account.weight > 0)
        .cloned()
        .collect();
    if positive_weight_accounts.is_empty() {
        return top_priority_accounts.into_iter().next();
    }

    let total_weight: i64 = positive_weight_accounts
        .iter()
        .map(|account| i64::from(account.weight))
        .sum();

    let mut pick = rand::rng().random_range(0..total_weight);
    for account in positive_weight_accounts {
        let weight = i64::from(account.weight);
        if pick < weight {
            return Some(account);
        }
        pick -= weight;
    }

    None
}
