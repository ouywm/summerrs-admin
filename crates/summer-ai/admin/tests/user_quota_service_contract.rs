use summer_ai_admin::service::user_quota_service::{
    apply_quota_delta, transaction_direction, transaction_trade_type,
};

#[test]
fn apply_quota_delta_adds_credit_to_total_quota() {
    assert_eq!(apply_quota_delta(10_000, 7_000, 2_500).unwrap(), 12_500);
}

#[test]
fn apply_quota_delta_rejects_debit_below_used_quota() {
    let err = apply_quota_delta(10_000, 7_000, -4_000).unwrap_err();
    assert!(err.contains("不能低于已使用额度"));
}

#[test]
fn transaction_direction_matches_delta_sign() {
    assert_eq!(transaction_direction(1), "credit");
    assert_eq!(transaction_direction(-1), "debit");
}

#[test]
fn transaction_trade_type_is_adjust() {
    assert_eq!(transaction_trade_type(), "adjust");
}
