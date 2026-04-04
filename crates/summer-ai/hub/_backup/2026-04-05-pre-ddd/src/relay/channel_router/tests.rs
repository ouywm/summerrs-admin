use super::*;

#[test]
fn select_from_route_candidates_falls_back_to_another_account_before_another_channel() {
    let mut exclusions = RouteSelectionExclusions::default();
    exclusions.exclude_account(101);

    let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

    assert_eq!(selected.channel_id, 11);
    assert_eq!(selected.account_id, 102);
}

#[test]
fn select_from_route_candidates_skips_excluded_channel() {
    let mut exclusions = RouteSelectionExclusions::default();
    exclusions.exclude_channel(11);

    let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

    assert_eq!(selected.channel_id, 12);
    assert_eq!(selected.account_id, 201);
}

#[test]
fn select_from_route_candidates_returns_none_when_all_accounts_are_excluded() {
    let mut exclusions = RouteSelectionExclusions::default();
    exclusions.exclude_account(101);
    exclusions.exclude_account(102);
    exclusions.exclude_channel(12);

    assert!(select_from_route_candidates(&sample_candidates(), &exclusions).is_none());
}

#[test]
fn selected_is_excluded_matches_channel_or_account() {
    let selected = SelectedChannel {
        channel_id: 11,
        channel_name: "primary".into(),
        channel_type: 1,
        base_url: "https://primary.example".into(),
        model_mapping: serde_json::json!({}),
        api_key: "sk-primary".into(),
        account_id: 101,
        account_name: "primary-a".into(),
    };

    let mut exclusions = RouteSelectionExclusions::default();
    assert!(!exclusions.selected_is_excluded(&selected));

    exclusions.exclude_account(101);
    assert!(exclusions.selected_is_excluded(&selected));

    let mut exclusions = RouteSelectionExclusions::default();
    exclusions.exclude_channel(11);
    assert!(exclusions.selected_is_excluded(&selected));
}

#[test]
fn select_from_route_candidates_falls_back_to_lower_priority_when_top_priority_is_exhausted() {
    let mut exclusions = RouteSelectionExclusions::default();
    exclusions.exclude_account(101);
    exclusions.exclude_account(102);

    let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

    assert_eq!(selected.channel_id, 12);
    assert_eq!(selected.account_id, 201);
}

#[test]
fn channel_supports_endpoint_scope_defaults_to_chat() {
    assert!(channel_supports_endpoint_scope(
        1,
        &serde_json::json!([]),
        "chat"
    ));
    assert!(!channel_supports_endpoint_scope(
        1,
        &serde_json::json!([]),
        "responses"
    ));
}

#[test]
fn channel_supports_endpoint_scope_respects_provider_allowlist() {
    assert!(channel_supports_endpoint_scope(
        3,
        &serde_json::json!(["chat", "responses"]),
        "responses"
    ));
    assert!(channel_supports_endpoint_scope(
        3,
        &serde_json::json!(["chat", "responses"]),
        "chat"
    ));
    assert!(!channel_supports_endpoint_scope(
        3,
        &serde_json::json!(["chat", "responses"]),
        "embeddings"
    ));
}

#[test]
fn channel_supports_endpoint_scope_keeps_azure_responses_available() {
    assert!(channel_supports_endpoint_scope(
        14,
        &serde_json::json!(["chat", "responses", "embeddings"]),
        "responses"
    ));
    assert!(channel_supports_endpoint_scope(
        14,
        &serde_json::json!(["chat", "responses", "embeddings"]),
        "embeddings"
    ));
}

#[test]
fn channel_supports_endpoint_scope_keeps_gemini_embeddings_available() {
    assert!(channel_supports_endpoint_scope(
        24,
        &serde_json::json!(["chat", "embeddings"]),
        "embeddings"
    ));
    assert!(!channel_supports_endpoint_scope(
        24,
        &serde_json::json!(["chat", "embeddings"]),
        "responses"
    ));
}

#[test]
fn merge_default_route_candidates_keeps_ability_candidates_and_adds_scope_fallbacks() {
    let merged = merge_default_route_candidates(
        vec![CachedRouteCandidate {
            channel_id: 11,
            channel_name: "ability".into(),
            channel_type: 1,
            base_url: "https://ability.example".into(),
            model_mapping: serde_json::json!({}),
            priority: 10,
            weight: 2,
            channel_failure_streak: 0,
            channel_response_time: 0,
            last_health_status: 1,
            recent_penalty_count: 0,
            recent_rate_limit_count: 0,
            recent_overload_count: 0,
            accounts: vec![CachedRouteAccount {
                account_id: 101,
                account_name: "ability-account".into(),
                weight: 1,
                priority: 10,
                failure_streak: 0,
                response_time: 0,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                api_key: "sk-ability".into(),
            }],
        }],
        vec![
            CachedRouteCandidate {
                channel_id: 11,
                channel_name: "fallback-dup".into(),
                channel_type: 1,
                base_url: "https://fallback-dup.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 1,
                weight: 1,
                channel_failure_streak: 0,
                channel_response_time: 0,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 111,
                    account_name: "fallback-dup-account".into(),
                    weight: 1,
                    priority: 1,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-fallback-dup".into(),
                }],
            },
            CachedRouteCandidate {
                channel_id: 12,
                channel_name: "fallback".into(),
                channel_type: 1,
                base_url: "https://fallback.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 5,
                weight: 1,
                channel_failure_streak: 0,
                channel_response_time: 0,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 201,
                    account_name: "fallback-account".into(),
                    weight: 1,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-fallback".into(),
                }],
            },
        ],
    );

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].channel_id, 11);
    assert_eq!(merged[0].accounts[0].account_id, 101);
    assert_eq!(merged[1].channel_id, 12);
}

#[test]
fn weighted_random_select_returns_first_item_when_total_weight_is_negative() {
    let picked = weighted_random_select(&[(11_i64, -3), (12_i64, -2)]);
    assert_eq!(picked, Some(11));
}

#[test]
fn select_from_route_candidates_returns_first_top_priority_candidate_when_weights_are_non_positive()
{
    let selected = select_from_route_candidates(
        &[
            CachedRouteCandidate {
                channel_id: 11,
                channel_name: "primary".into(),
                channel_type: 1,
                base_url: "https://primary.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: -5,
                channel_failure_streak: 0,
                channel_response_time: 0,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 101,
                    account_name: "primary-account".into(),
                    weight: 1,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-primary".into(),
                }],
            },
            CachedRouteCandidate {
                channel_id: 12,
                channel_name: "fallback".into(),
                channel_type: 1,
                base_url: "https://fallback.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: -1,
                channel_failure_streak: 0,
                channel_response_time: 0,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 201,
                    account_name: "fallback-account".into(),
                    weight: 1,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-fallback".into(),
                }],
            },
        ],
        &RouteSelectionExclusions::default(),
    )
    .unwrap();

    assert_eq!(selected.channel_id, 11);
    assert_eq!(selected.account_id, 101);
}

#[test]
fn select_from_route_candidates_prefers_healthier_channel_when_priority_matches() {
    let selected = select_from_route_candidates(
        &[
            CachedRouteCandidate {
                channel_id: 11,
                channel_name: "degraded".into(),
                channel_type: 1,
                base_url: "https://degraded.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 0,
                channel_failure_streak: 4,
                channel_response_time: 800,
                last_health_status: 3,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 101,
                    account_name: "degraded-account".into(),
                    weight: 0,
                    priority: 10,
                    failure_streak: 3,
                    response_time: 500,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-degraded".into(),
                }],
            },
            CachedRouteCandidate {
                channel_id: 12,
                channel_name: "healthy".into(),
                channel_type: 1,
                base_url: "https://healthy.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 0,
                channel_failure_streak: 0,
                channel_response_time: 80,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 201,
                    account_name: "healthy-account".into(),
                    weight: 0,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 80,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-healthy".into(),
                }],
            },
        ],
        &RouteSelectionExclusions::default(),
    )
    .unwrap();

    assert_eq!(selected.channel_id, 12);
    assert_eq!(selected.account_id, 201);
}

#[test]
fn pick_schedulable_account_prefers_healthier_account_when_priority_matches() {
    let first = CachedRouteAccount {
        account_id: 101,
        account_name: "degraded-account".into(),
        weight: 0,
        priority: 10,
        failure_streak: 5,
        response_time: 600,
        recent_penalty_count: 0,
        recent_rate_limit_count: 0,
        recent_overload_count: 0,
        api_key: "sk-degraded".into(),
    };
    let second = CachedRouteAccount {
        account_id: 102,
        account_name: "healthy-account".into(),
        weight: 0,
        priority: 10,
        failure_streak: 0,
        response_time: 90,
        recent_penalty_count: 0,
        recent_rate_limit_count: 0,
        recent_overload_count: 0,
        api_key: "sk-healthy".into(),
    };

    let accounts = [&first, &second];
    let selected = pick_schedulable_account(&accounts).expect("selected account");

    assert_eq!(selected.account_id, 102);
}

#[test]
fn select_from_route_candidates_prefers_lower_recent_penalty_when_persistent_health_ties() {
    let selected = select_from_route_candidates(
        &[
            CachedRouteCandidate {
                channel_id: 11,
                channel_name: "flaky".into(),
                channel_type: 1,
                base_url: "https://flaky.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 0,
                channel_failure_streak: 0,
                channel_response_time: 90,
                last_health_status: 1,
                recent_penalty_count: 3,
                recent_rate_limit_count: 1,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 101,
                    account_name: "flaky-account".into(),
                    weight: 0,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 90,
                    recent_penalty_count: 2,
                    recent_rate_limit_count: 1,
                    recent_overload_count: 0,
                    api_key: "sk-flaky".into(),
                }],
            },
            CachedRouteCandidate {
                channel_id: 12,
                channel_name: "stable".into(),
                channel_type: 1,
                base_url: "https://stable.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 0,
                channel_failure_streak: 0,
                channel_response_time: 90,
                last_health_status: 1,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 201,
                    account_name: "stable-account".into(),
                    weight: 0,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 90,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-stable".into(),
                }],
            },
        ],
        &RouteSelectionExclusions::default(),
    )
    .unwrap();

    assert_eq!(selected.channel_id, 12);
    assert_eq!(selected.account_id, 201);
}

#[test]
fn pick_schedulable_account_prefers_lower_recent_penalty_when_persistent_health_ties() {
    let first = CachedRouteAccount {
        account_id: 101,
        account_name: "flaky-account".into(),
        weight: 0,
        priority: 10,
        failure_streak: 0,
        response_time: 90,
        recent_penalty_count: 3,
        recent_rate_limit_count: 1,
        recent_overload_count: 0,
        api_key: "sk-flaky".into(),
    };
    let second = CachedRouteAccount {
        account_id: 102,
        account_name: "stable-account".into(),
        weight: 0,
        priority: 10,
        failure_streak: 0,
        response_time: 90,
        recent_penalty_count: 0,
        recent_rate_limit_count: 0,
        recent_overload_count: 0,
        api_key: "sk-stable".into(),
    };

    let accounts = [&first, &second];
    let selected = pick_schedulable_account(&accounts).expect("selected account");

    assert_eq!(selected.account_id, 102);
}

#[test]
fn compute_route_cache_ttl_seconds_defaults_to_base_ttl_without_refresh_deadline() {
    let now = chrono::Utc::now().fixed_offset();

    assert_eq!(
        compute_route_cache_ttl_seconds(now, None),
        ROUTE_CACHE_TTL_SECONDS
    );
}

#[test]
fn compute_route_cache_ttl_seconds_uses_earliest_refresh_deadline() {
    let now = chrono::Utc::now().fixed_offset();

    assert_eq!(
        compute_route_cache_ttl_seconds(now, Some(now + chrono::Duration::seconds(12))),
        12
    );
    assert_eq!(
        compute_route_cache_ttl_seconds(now, Some(now + chrono::Duration::milliseconds(400))),
        1
    );
}

#[test]
fn weighted_random_select_supports_large_positive_weights_without_overflow() {
    let picked = weighted_random_select(&[(11_i64, i32::MAX), (12_i64, i32::MAX)]);

    assert!(matches!(picked, Some(11 | 12)));
}

#[test]
fn build_route_plan_from_candidates_keeps_request_local_fallback_order_stable() {
    let plan = build_route_plan_from_candidates(
        &sample_candidates(),
        &RouteSelectionExclusions::default(),
    );

    let ordered: Vec<(i64, i64)> = plan
        .into_iter()
        .map(|selected| (selected.channel_id, selected.account_id))
        .collect();

    assert_eq!(ordered, vec![(11, 101), (11, 102), (12, 201)]);
}

#[test]
fn route_selection_plan_skips_future_entries_for_failed_channel() {
    let mut plan = RouteSelectionPlan::new(
        build_route_plan_from_candidates(
            &sample_candidates(),
            &RouteSelectionExclusions::default(),
        ),
        RouteSelectionExclusions::default(),
    );

    let first = plan.next().expect("first route");
    assert_eq!((first.channel_id, first.account_id), (11, 101));

    plan.exclude_selected_channel(&first);

    let second = plan.next().expect("second route");
    assert_eq!((second.channel_id, second.account_id), (12, 201));
    assert!(plan.next().is_none());
}

fn sample_candidates() -> Vec<CachedRouteCandidate> {
    vec![
        CachedRouteCandidate {
            channel_id: 11,
            channel_name: "primary".into(),
            channel_type: 1,
            base_url: "https://primary.example".into(),
            model_mapping: serde_json::json!({}),
            priority: 10,
            weight: 1,
            channel_failure_streak: 0,
            channel_response_time: 0,
            last_health_status: 1,
            recent_penalty_count: 0,
            recent_rate_limit_count: 0,
            recent_overload_count: 0,
            accounts: vec![
                CachedRouteAccount {
                    account_id: 101,
                    account_name: "primary-a".into(),
                    weight: 1,
                    priority: 10,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-primary-a".into(),
                },
                CachedRouteAccount {
                    account_id: 102,
                    account_name: "primary-b".into(),
                    weight: 1,
                    priority: 8,
                    failure_streak: 0,
                    response_time: 0,
                    recent_penalty_count: 0,
                    recent_rate_limit_count: 0,
                    recent_overload_count: 0,
                    api_key: "sk-primary-b".into(),
                },
            ],
        },
        CachedRouteCandidate {
            channel_id: 12,
            channel_name: "secondary".into(),
            channel_type: 1,
            base_url: "https://secondary.example".into(),
            model_mapping: serde_json::json!({}),
            priority: 5,
            weight: 0,
            channel_failure_streak: 0,
            channel_response_time: 0,
            last_health_status: 1,
            recent_penalty_count: 0,
            recent_rate_limit_count: 0,
            recent_overload_count: 0,
            accounts: vec![CachedRouteAccount {
                account_id: 201,
                account_name: "secondary-a".into(),
                weight: 1,
                priority: 10,
                failure_streak: 0,
                response_time: 0,
                recent_penalty_count: 0,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
                api_key: "sk-secondary-a".into(),
            }],
        },
    ]
}
