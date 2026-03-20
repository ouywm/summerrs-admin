//! Socket.IO 模块
//!
//! - `core/`    — 公共定义：事件名、数据模型、Room 名构造
//! - `gateway/` — 连接网关：认证、会话存储、断连推送

pub mod core;
pub mod connection;

// ── 兼容重导出 ──────────────────────────────────────────────
// 保持外部 `use crate::socketio::service::{...}` 路径不变
pub use connection::service;
// 保持外部 `use crate::socketio::room` 路径不变
pub use core::room;
