use schemars::JsonSchema;
use serde::Serialize;

/// 在线用户条目
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineUserVo {
    /// 用户 ID
    pub user_id: i64,
    /// 用户名
    pub user_name: String,
    /// 昵称
    pub nick_name: String,
    /// 头像
    pub avatar: String,
    /// 设备类型
    pub device: String,
    /// 登录时间（Unix 毫秒时间戳）
    pub login_time: i64,
    /// 登录 IP
    pub login_ip: String,
    /// 登录位置
    pub login_location: String,
}
