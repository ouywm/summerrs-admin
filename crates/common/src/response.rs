use serde::Serialize;
use summer_web::axum::response::IntoResponse;
use summer_web::axum::{self, Json};

/// 业务成功码
const SUCCESS_CODE: i32 = 200;

/// 统一成功响应包装，始终返回 {"code": ..., "msg": "...", "data": ...}
#[derive(Debug, Serialize, schemars::JsonSchema)]
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

/// 为 aide OpenAPI 文档生成提供支持
impl<T: Serialize + schemars::JsonSchema> summer_web::aide::OperationOutput for ApiResponse<T> {
    type Inner = Self;

    fn operation_response(
        ctx: &mut summer_web::aide::generate::GenContext,
        _operation: &mut summer_web::aide::openapi::Operation,
    ) -> Option<summer_web::aide::openapi::Response> {
        let json_schema = ctx.schema.subschema_for::<Self>();
        let resolved = ctx.resolve_schema(&json_schema);

        Some(summer_web::aide::openapi::Response {
            description: resolved
                .get("description")
                .and_then(|d| d.as_str())
                .map(String::from)
                .unwrap_or_default(),
            content: indexmap::IndexMap::from_iter([(
                "application/json".into(),
                summer_web::aide::openapi::MediaType {
                    schema: Some(summer_web::aide::openapi::SchemaObject {
                        json_schema,
                        example: None,
                        external_docs: None,
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
    }

    fn inferred_responses(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) -> Vec<(
        Option<summer_web::aide::openapi::StatusCode>,
        summer_web::aide::openapi::Response,
    )> {
        if let Some(res) = Self::operation_response(ctx, operation) {
            vec![(Some(summer_web::aide::openapi::StatusCode::Code(200)), res)]
        } else {
            vec![]
        }
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
