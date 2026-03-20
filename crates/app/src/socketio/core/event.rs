/// Socket.IO 事件名集中定义
///
/// 所有服务端推送的事件名都在此模块中以常量形式声明，
/// 避免字符串字面量散落在业务代码中。

/// 强制下线通知，payload: [`super::model::KickoutPayload`]
pub const SESSION_KICKOUT: &str = "session.kickout";
