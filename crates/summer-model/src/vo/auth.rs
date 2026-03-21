use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginVo {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

/// 设备会话信息
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionVo {
    /// 设备类型：web / android / ios / ...
    pub device: String,
    /// 登录时间（Unix 时间戳）
    pub login_time: i64,
    /// 登录 IP
    pub login_ip: String,
    /// 浏览器信息
    pub browser: String,
    /// 操作系统
    pub os: String,
}
