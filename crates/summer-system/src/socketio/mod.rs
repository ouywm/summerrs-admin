//! Socket.IO 模块
//!
//! - `core/`       — 公共定义：事件名、数据模型、Room 名构造、通用推送服务
//! - `connection/` — 连接网关：认证、会话存储、断连推送

pub mod connection;
pub mod core;
pub use connection::service;
pub use core::room;
#[allow(unused_imports)]
pub use core::emitter::SocketEmitter;
