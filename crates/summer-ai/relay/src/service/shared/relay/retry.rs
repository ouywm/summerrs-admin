use summer_ai_model::entity::retry_attempt::{self, RetryAttemptStatus};

use crate::service::tracking::{CreateRetryAttemptTracking, TrackingService};

pub(crate) const RELAY_MAX_UPSTREAM_ATTEMPTS: i32 = 3;
const RELAY_RETRY_DOMAIN_CODE: &str = "relay";

pub(crate) fn build_retry_attempt_payload(
    execution_id: Option<i64>,
    channel_id: i64,
    account_id: i64,
    upstream_model: &str,
    status_code: i32,
    stage: &str,
    upstream_request_id: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "executionId": execution_id.unwrap_or_default(),
        "channelId": channel_id,
        "accountId": account_id,
        "upstreamModel": upstream_model,
        "statusCode": status_code,
        "stage": stage,
        "upstreamRequestId": upstream_request_id.unwrap_or_default(),
    })
}

pub(crate) async fn create_relay_retry_attempt(
    tracking: &TrackingService,
    task_type: &str,
    request_id: &str,
    attempt_no: i32,
    error_message: &str,
    payload: serde_json::Value,
) -> Option<retry_attempt::Model> {
    match tracking
        .create_retry_attempt(CreateRetryAttemptTracking {
            domain_code: RELAY_RETRY_DOMAIN_CODE,
            task_type,
            reference_id: request_id,
            request_id,
            attempt_no,
            backoff_seconds: 0,
            error_message,
            payload,
            next_retry_at: None,
        })
        .await
    {
        Ok(model) => Some(model),
        Err(error) => {
            tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry_attempt tracking row");
            None
        }
    }
}

pub(crate) async fn finish_relay_retry_attempt(
    tracking: &TrackingService,
    request_id: &str,
    retry_attempt: Option<&retry_attempt::Model>,
    status: RetryAttemptStatus,
    error_message: &str,
    payload: serde_json::Value,
) {
    let Some(retry_attempt) = retry_attempt else {
        return;
    };

    if let Err(error) = tracking
        .finish_retry_attempt(retry_attempt.id, status, error_message, payload)
        .await
    {
        tracing::warn!(
            request_id,
            retry_attempt_id = retry_attempt.id,
            error = %error,
            "failed to finalize retry_attempt tracking row"
        );
    }
}

pub(crate) async fn advance_relay_retry(
    tracking: &TrackingService,
    task_type: &str,
    request_id: &str,
    attempt_no: i32,
    pending_retry_attempt: &mut Option<retry_attempt::Model>,
    error_message: &str,
    payload: serde_json::Value,
    retryable: bool,
) -> bool {
    let should_retry = retryable && attempt_no < RELAY_MAX_UPSTREAM_ATTEMPTS;

    if attempt_no > 1 {
        finish_relay_retry_attempt(
            tracking,
            request_id,
            pending_retry_attempt.as_ref(),
            if should_retry {
                RetryAttemptStatus::Failed
            } else {
                RetryAttemptStatus::Abandoned
            },
            error_message,
            payload.clone(),
        )
        .await;
    }

    if should_retry {
        *pending_retry_attempt = create_relay_retry_attempt(
            tracking,
            task_type,
            request_id,
            attempt_no,
            error_message,
            payload,
        )
        .await;
        return true;
    }

    false
}

pub(crate) async fn complete_relay_retry_success(
    tracking: &TrackingService,
    request_id: &str,
    pending_retry_attempt: Option<&retry_attempt::Model>,
    payload: serde_json::Value,
) {
    finish_relay_retry_attempt(
        tracking,
        request_id,
        pending_retry_attempt,
        RetryAttemptStatus::Succeeded,
        pending_retry_attempt
            .map(|model| model.error_message.as_str())
            .unwrap_or_default(),
        payload,
    )
    .await;
}
