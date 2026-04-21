//! 追踪输出类型：一次请求的"最终结果"描述。
//!
//! 提到 "tracking" 的地方都围绕一次 relay 请求的完整生命周期 —— 从 handler 入口
//! 到响应返回客户端（流式场景为 stream 完结）。handler 只需**最后调一次**
//! [`TrackingService::emit`]（单 attempt）或 [`TrackingService::emit_with_attempts`]
//! （P9 retry，多 attempt），所有数据库落库在 `tokio::spawn` 内异步完成，不阻塞响应。
//!
//! **刻意极简**：v1 不做 "正在处理中" 状态查看（只写终态记录），避免 begin/finish
//! 双阶段带来的幂等 / 时序 / FK 映射问题。
//!
//! - **成功** → [`TrackingOutcome::Success`]：带 usage + 响应片段摘要
//! - **失败** → [`TrackingOutcome::Failure`]：带 status + 错误摘要
//! - **多 attempt 快照** → [`AttemptRecord`]：每次上游尝试一条，用于写 `ai.request_execution`

use sea_orm::prelude::DateTimeWithTimeZone;
use serde_json::Value;
use summer_ai_core::Usage;

/// 一次上游尝试的快照——对应 `ai.request_execution` 表的一行。
///
/// retry loop 每走一次（成功或失败）push 一条 `AttemptRecord`；pipeline 最后调
/// [`crate::service::tracking::TrackingService::emit_with_attempts`] 一次性落库：
///
/// - 1 条 `ai.request`（终态）
/// - N 条 `ai.request_execution`（按 attempt_no 顺序，1..=N）
/// - 1 条 `ai.log`（终态；`execution_id` 指向**最后一次** attempt 对应的 execution id）
///
/// 用 plain struct 而不是 `Set<ActiveModel>` 是因为构造侧（pipeline）不想 leak sea_orm
/// 的 `Set`——TrackingService 内部才把它变成 ActiveModel。
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub attempt_no: i32,
    pub channel_id: i64,
    pub account_id: i64,
    /// 上游协议标识（`adapter_kind.as_str()`）。
    pub request_format: String,
    /// 本次尝试发给上游的模型（可能和 `ctx.logical_model` 不同——channel.model_mapping）。
    pub upstream_model: String,
    /// 从上游响应 header 抽到的 request-id（可空）。
    pub upstream_request_id: Option<String>,
    /// 出站 headers 脱敏快照。
    pub sent_headers: Value,
    /// 发给上游的真实 request body（已翻译好的 canonical 或各家 wire）。
    pub request_body: Option<Value>,
    /// 上游响应 body 快照（失败也填——包括 4xx/5xx 的错误 JSON）。
    pub response_body: Option<Value>,
    pub response_status_code: i32,
    pub success: bool,
    pub error_message: String,
    pub duration_ms: i32,
    pub first_token_ms: i32,
    pub started_at: DateTimeWithTimeZone,
    pub finished_at: DateTimeWithTimeZone,
}

/// 一次请求的最终结果。
#[derive(Debug, Clone)]
pub enum TrackingOutcome {
    /// 上游返回成功（2xx），拿到完整 usage。
    Success {
        /// 上游 HTTP status。
        upstream_status: u16,
        /// Token 用量（非流式直接取响应；流式由 stream_driver 在终态收集）。
        usage: Usage,
        /// 返回给客户端的 body 摘要（非流式是完整 JSON；流式通常 None 或聚合片段）。
        response_snapshot: Option<serde_json::Value>,
        /// 上游返回的 request id（从响应 header 里提取，可空）。
        upstream_request_id: Option<String>,
    },
    /// 失败——任何让 handler 不得不返错误给客户端的场景。
    Failure {
        /// 最终返给客户端的 HTTP status（401/402/403/429/500/...）。
        client_status: u16,
        /// 上游 status（如果已经发出上游请求），未发过为 None。
        upstream_status: Option<u16>,
        /// 错误摘要（给审计用，不给客户端）。
        message: String,
        /// 上游返回的 body 快照（JSON 化）。已发请求但未拿到上游 body（例如 DNS
        /// 失败 / 连接超时）时为 None；非 JSON body 包成 `{"raw": "..."}`。
        response_snapshot: Option<serde_json::Value>,
    },
}

impl TrackingOutcome {
    /// 是否成功（给 DB 的 status 列）。
    pub fn is_success(&self) -> bool {
        matches!(self, TrackingOutcome::Success { .. })
    }

    /// 客户端最终看到的 status code。
    pub fn client_status(&self) -> u16 {
        match self {
            TrackingOutcome::Success { .. } => 200,
            TrackingOutcome::Failure { client_status, .. } => *client_status,
        }
    }

    /// 上游 status（没调上游或未知为 0）。
    pub fn upstream_status(&self) -> u16 {
        match self {
            TrackingOutcome::Success {
                upstream_status, ..
            } => *upstream_status,
            TrackingOutcome::Failure {
                upstream_status, ..
            } => upstream_status.unwrap_or(0),
        }
    }

    /// 错误摘要（成功场景为空串）。
    pub fn error_message(&self) -> &str {
        match self {
            TrackingOutcome::Success { .. } => "",
            TrackingOutcome::Failure { message, .. } => message,
        }
    }

    /// 取 Usage（失败场景返 0 的空 Usage）。
    pub fn usage(&self) -> Usage {
        match self {
            TrackingOutcome::Success { usage, .. } => usage.clone(),
            TrackingOutcome::Failure { .. } => Usage::default(),
        }
    }

    /// 响应快照（成功：拿成功响应；失败：拿上游返回的 body —— 能看到真实错误 JSON）。
    pub fn response_snapshot(&self) -> Option<serde_json::Value> {
        match self {
            TrackingOutcome::Success {
                response_snapshot, ..
            } => response_snapshot.clone(),
            TrackingOutcome::Failure {
                response_snapshot, ..
            } => response_snapshot.clone(),
        }
    }

    /// 上游 request id（如果有）。
    pub fn upstream_request_id(&self) -> Option<&str> {
        match self {
            TrackingOutcome::Success {
                upstream_request_id,
                ..
            } => upstream_request_id.as_deref(),
            TrackingOutcome::Failure { .. } => None,
        }
    }
}

/// 便利构造 —— 成功场景。
pub fn success(usage: Usage, upstream_status: u16) -> TrackingOutcome {
    TrackingOutcome::Success {
        upstream_status,
        usage,
        response_snapshot: None,
        upstream_request_id: None,
    }
}

/// 便利构造 —— 失败场景（只有客户端 status）。
pub fn failure(client_status: u16, message: impl Into<String>) -> TrackingOutcome {
    TrackingOutcome::Failure {
        client_status,
        upstream_status: None,
        message: message.into(),
        response_snapshot: None,
    }
}
