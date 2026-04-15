use serde::{Deserialize, Serialize};

use crate::user_type::{DeviceType, LoginId};

/// 用户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_name: String,
    pub nick_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl UserProfile {
    /// 获取昵称
    pub fn nick_name(&self) -> &str {
        &self.nick_name
    }

    /// 获取用户名
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    /// 获取角色列表
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// 获取权限列表
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
}

/// 用户会话（从 Access JWT claims 构造，注入到 request extensions）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub login_id: LoginId,
    /// 当前请求的设备类型
    pub device: DeviceType,
    /// 当前登录用户的档案信息
    pub profile: UserProfile,
}

/// Redis 设备登录信息（存储在 `auth:device:{login_id}:{device}`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Refresh Token 的 UUID（rid）
    pub rid: String,
    /// 登录时间（Unix 时间戳）
    pub login_time: i64,
    /// 登录 IP
    pub login_ip: String,
    /// 用户代理信息
    pub user_agent: String,
}

/// 设备会话（用于在线用户查询返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSession {
    /// 设备标识
    pub device: DeviceType,
    /// 登录时间（Unix 时间戳）
    pub login_time: i64,
    /// 登录 IP
    pub login_ip: String,
    /// 用户代理信息
    pub user_agent: String,
}

/// `validate_token` 返回的验证结果（从 Access JWT claims 提取）
#[derive(Debug, Clone)]
pub struct ValidatedAccess {
    pub login_id: LoginId,
    pub device: DeviceType,
    pub user_name: String,
    pub nick_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}
