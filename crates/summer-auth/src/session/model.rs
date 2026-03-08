use serde::{Deserialize, Serialize};

use crate::user_type::{DeviceType, LoginId};

/// Admin 用户档案（含 RBAC）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminProfile {
    pub user_name: String,
    pub nick_name: String,
    pub avatar: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// Business 用户档案（含 RBAC）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessProfile {
    pub user_name: String,
    pub nick_name: String,
    pub avatar: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// Customer 用户档案（无 RBAC）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerProfile {
    pub nick_name: String,
    pub avatar: String,
}

/// 用户档案枚举（按用户类型区分字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UserProfile {
    Admin(AdminProfile),
    Business(BusinessProfile),
    Customer(CustomerProfile),
}

impl UserProfile {
    /// 获取昵称（所有用户类型通用）
    pub fn nick_name(&self) -> &str {
        match self {
            UserProfile::Admin(p) => &p.nick_name,
            UserProfile::Business(p) => &p.nick_name,
            UserProfile::Customer(p) => &p.nick_name,
        }
    }

    /// 获取头像（所有用户类型通用）
    pub fn avatar(&self) -> &str {
        match self {
            UserProfile::Admin(p) => &p.avatar,
            UserProfile::Business(p) => &p.avatar,
            UserProfile::Customer(p) => &p.avatar,
        }
    }

    /// 获取用户名（Customer 无 user_name，返回空字符串）
    pub fn user_name(&self) -> &str {
        match self {
            UserProfile::Admin(p) => &p.user_name,
            UserProfile::Business(p) => &p.user_name,
            UserProfile::Customer(_) => "",
        }
    }

    /// 获取角色列表（Customer 无 RBAC，返回空 slice）
    pub fn roles(&self) -> &[String] {
        match self {
            UserProfile::Admin(p) => &p.roles,
            UserProfile::Business(p) => &p.roles,
            UserProfile::Customer(_) => &[],
        }
    }

    /// 获取权限列表（Customer 无 RBAC，返回空 slice）
    pub fn permissions(&self) -> &[String] {
        match self {
            UserProfile::Admin(p) => &p.permissions,
            UserProfile::Business(p) => &p.permissions,
            UserProfile::Customer(_) => &[],
        }
    }

    /// 设置角色列表（仅 Admin/Business 有效，Customer 忽略）
    pub fn set_roles(&mut self, roles: Vec<String>) {
        match self {
            UserProfile::Admin(p) => p.roles = roles,
            UserProfile::Business(p) => p.roles = roles,
            UserProfile::Customer(_) => {}
        }
    }

    /// 设置权限列表（仅 Admin/Business 有效，Customer 忽略）
    pub fn set_permissions(&mut self, perms: Vec<String>) {
        match self {
            UserProfile::Admin(p) => p.permissions = perms,
            UserProfile::Business(p) => p.permissions = perms,
            UserProfile::Customer(_) => {}
        }
    }
}

/// 用户会话（一个用户可有多个设备会话）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub login_id: LoginId,
    /// 每个设备一个 DeviceSession
    pub devices: Vec<DeviceSession>,
    /// 用户档案（按用户类型区分字段）
    pub profile: UserProfile,
}

/// 设备会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSession {
    /// 设备标识
    pub device: DeviceType,
    /// access token
    pub access_token: String,
    /// refresh token
    pub refresh_token: String,
    /// 登录时间（Unix 时间戳）
    pub login_time: i64,
    /// 最后活跃时间
    pub last_active_time: i64,
    /// 登录 IP
    pub login_ip: String,
    /// 用户代理信息
    pub user_agent: String,
    /// JWT 模式下 access token 的 JTI（opaque 模式为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_jti: Option<String>,
    /// JWT 模式下 refresh token 的 JTI（opaque 模式为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_jti: Option<String>,
}
