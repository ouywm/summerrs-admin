use bytes::Bytes;
use serde_json::Value;
use summer_web::axum::http::{
    HeaderMap, HeaderName, HeaderValue, StatusCode, header::CONTENT_TYPE,
};
use summer_web::axum::response::{IntoResponse, Response};

use crate::service::openai_http::insert_request_id_header;

pub(crate) fn build_bytes_response(
    status: StatusCode,
    body: Bytes,
    content_type: Option<HeaderValue>,
    request_id: &str,
) -> Response {
    let mut response = (status, body).into_response();
    if let Some(content_type) = content_type {
        response.headers_mut().insert(CONTENT_TYPE, content_type);
    }
    insert_request_id_header(&mut response, request_id);
    response
}

pub(crate) fn unusable_success_response_message(
    status: StatusCode,
    body: &Bytes,
    endpoint: &str,
    allow_empty_body: bool,
) -> Option<String> {
    if !status.is_success() {
        return None;
    }

    if status != StatusCode::NO_CONTENT
        && !allow_empty_body
        && body.iter().all(|byte| byte.is_ascii_whitespace())
    {
        return Some(format!(
            "upstream returned an empty success response for endpoint {endpoint}"
        ));
    }

    let payload = serde_json::from_slice::<Value>(body).ok()?;
    let message = detect_unusable_upstream_success_response(&payload)?;
    Some(format!(
        "upstream returned an unusable success response for endpoint {endpoint}: {message}"
    ))
}

pub(crate) fn detect_unusable_upstream_success_response(payload: &Value) -> Option<String> {
    let error = payload.get("error")?;
    if !error.is_object() {
        return None;
    }

    error
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            error
                .get("code")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            error
                .get("type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

pub(crate) fn allow_empty_success_body_for_upstream_path(upstream_path: &str) -> bool {
    upstream_path.starts_with("/v1/files/") && upstream_path.ends_with("/content")
}

pub(crate) fn apply_forward_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
    preserve_content_type: bool,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if should_forward_header(name, preserve_content_type) {
            builder = builder.header(name, value.clone());
        }
    }
    builder
}

pub(crate) fn should_forward_header(name: &HeaderName, preserve_content_type: bool) -> bool {
    if !preserve_content_type && name == CONTENT_TYPE {
        return false;
    }

    !matches!(
        name.as_str(),
        "authorization"
            | "content-length"
            | "host"
            | "connection"
            | "transfer-encoding"
            | "content-encoding"
    )
}

pub(crate) fn apply_upstream_auth(
    builder: reqwest::RequestBuilder,
    channel_type: i16,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match channel_type {
        14 => builder.header("api-key", api_key),
        _ => builder.bearer_auth(api_key),
    }
}

pub(crate) fn build_upstream_url(base_url: &str, path: &str, query: Option<&str>) -> String {
    let base_url = base_url.trim_end_matches('/');
    let path = if base_url.ends_with("/openai/v1") && path.starts_with("/v1/") {
        &path[3..]
    } else {
        path
    };
    let mut url = format!("{base_url}{path}");
    if let Some(query) = query.filter(|query| !query.is_empty()) {
        url.push('?');
        url.push_str(query);
    }
    url
}
