use serde::Serialize;
use spring_web::axum::response::IntoResponse;
use spring_web::axum::{self, Json};

fn is_zero(v: &i32) -> bool {
    *v == 0
}

fn is_empty(v: &str) -> bool {
    v.is_empty()
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    #[serde(skip_serializing_if = "is_zero")]
    pub code: i32,
    pub data: T,
    #[serde(skip_serializing_if = "is_empty")]
    pub message: String,
}

impl<T: Serialize> ApiResponse<T> {
    /// Success with data only → {"data": ...}
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            data,
            message: String::new(),
        }
    }

    /// Success with data and message → {"data": ..., "message": "xxx"}
    pub fn ok_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            code: 0,
            data,
            message: message.into(),
        }
    }

    /// Business warning with code + data + message → {"code": 1001, "data": ..., "message": "xxx"}
    pub fn warn(code: i32, data: T, message: impl Into<String>) -> Self {
        Self {
            code,
            data,
            message: message.into(),
        }
    }

    /// Business warning with code + data, no message → {"code": 1001, "data": ...}
    pub fn warn_code(code: i32, data: T) -> Self {
        Self {
            code,
            data,
            message: String::new(),
        }
    }
}

impl ApiResponse<()> {
    /// Success with no data → {"data": null}
    pub fn empty() -> Self {
        Self {
            code: 0,
            data: (),
            message: String::new(),
        }
    }

    /// Success with no data but with message → {"data": null, "message": "xxx"}
    pub fn empty_with_message(message: impl Into<String>) -> Self {
        Self {
            code: 0,
            data: (),
            message: message.into(),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}