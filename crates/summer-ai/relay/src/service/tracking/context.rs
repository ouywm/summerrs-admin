//! 追踪输出类型：一次请求的"最终结果"描述。
//!
//! 提到 "tracking" 的地方都围绕一次 relay 请求的完整生命周期 —— 从 handler 入口
//! 到响应返回客户端（流式场景为 stream 完结）。handler 只需**最后调一次**
//! [`TrackingService::emit`]，所有数据库落库在 `tokio::spawn` 内异步完成，不阻
//! 塞响应。
//!
//! **刻意极简**：v1 不做 "正在处理中" 状态查看（只写终态记录），避免 begin/finish
//! 双阶段带来的幂等 / 时序 / FK 映射问题。
//!
//! - **成功** → [`TrackingOutcome::Success`]：带 usage + 响应片段摘要
//! - **失败** → [`TrackingOutcome::Failure`]：带 status + 错误摘要

use summer_ai_core::Usage;

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

    /// 响应快照（失败场景总是 None）。
    pub fn response_snapshot(&self) -> Option<serde_json::Value> {
        match self {
            TrackingOutcome::Success {
                response_snapshot, ..
            } => response_snapshot.clone(),
            TrackingOutcome::Failure { .. } => None,
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
    }
}
