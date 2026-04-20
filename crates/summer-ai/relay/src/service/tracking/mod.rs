//! 请求追踪（tracking）—— 把每个 relay 请求完结时的状态一次性落库到 3 表：
//!
//! - `ai.request`           —— 主表（客户端视角的一次完整请求）
//! - `ai.request_execution` —— 每次上游尝试（v1 固定 attempt_no=1，P9 retry 后多次）
//! - `ai.log`               —— 消费摘要（UI 列表、计费核对用）
//!
//! # 用法
//!
//! handler 在**返回响应前**一次调用 [`TrackingService::emit`]，带上最终的
//! [`TrackingOutcome`]（成功/失败）、请求 body 快照。服务内部 `tokio::spawn` 异步
//! 落库，DB 慢不影响响应。
//!
//! # 与 billing 的耦合
//!
//! 本模块**只负责写日志**，不算钱。P6 的 `BillingService::settle` 在扣费完成后
//! 通过 [`TrackingService::update_cost_by_request_id`] 回填 `log.quota` +
//! `log.cost_total` + `log.price_reference`。

pub mod context;
pub mod service;

pub use context::{TrackingOutcome, failure, success};
pub use service::TrackingService;
