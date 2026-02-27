use serde::Serialize;
use spring_web::axum::response::IntoResponse;
use spring_web::axum::{self, Json};

/// 业务成功码
const SUCCESS_CODE: i32 = 200;

/// 统一成功响应包装，始终返回 {"code": ..., "msg": "...", "data": ...}
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: i32,
    pub msg: String,
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    /// 成功 → {"code": 200, "msg": "", "data": ...}
    pub fn ok(data: T) -> Self {
        Self {
            code: SUCCESS_CODE,
            msg: String::new(),
            data,
        }
    }

    /// 成功 + 提示消息 → {"code": 200, "msg": "xxx", "data": ...}
    pub fn ok_with_msg(data: T, msg: impl Into<String>) -> Self {
        Self {
            code: SUCCESS_CODE,
            msg: msg.into(),
            data,
        }
    }

    /// 业务警告 → {"code": 1001, "msg": "xxx", "data": ...}
    pub fn warn(code: i32, data: T, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
            data,
        }
    }

    /// 业务警告（无消息） → {"code": 1001, "msg": "", "data": ...}
    pub fn warn_code(code: i32, data: T) -> Self {
        Self {
            code,
            msg: String::new(),
            data,
        }
    }
}

impl ApiResponse<()> {
    /// 成功（无数据） → {"code": 200, "msg": "", "data": null}
    pub fn empty() -> Self {
        Self {
            code: SUCCESS_CODE,
            msg: String::new(),
            data: (),
        }
    }

    /// 成功（无数据）+ 提示消息 → {"code": 200, "msg": "xxx", "data": null}
    pub fn empty_with_msg(msg: impl Into<String>) -> Self {
        Self {
            code: SUCCESS_CODE,
            msg: msg.into(),
            data: (),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

/// 分页响应，配合 ApiResponse 使用：ApiResponse<PageResponse<T>>
///
/// 输出格式：
/// ```json
/// {
///   "code": 200,
///   "msg": "",
///   "data": {
///     "records": [...],
///     "current": 1,
///     "size": 10,
///     "total": 56
///   }
/// }
/// ```
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub records: Vec<T>,
    pub current: u64,
    pub size: u64,
    pub total: u64,
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(records: Vec<T>, current: u64, size: u64, total: u64) -> Self {
        Self {
            records,
            current,
            size,
            total,
        }
    }
}
