use serde::{Deserialize, Serialize};

use crate::user_type::{DeviceType, LoginId};

/// UUID 模式的 Redis session 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UuidSessionData {
    pub login_id: String,
    pub device: String,
    pub iat: i64,
    pub user_name: String,
    pub nick_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

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
}

/// 用户会话（从 Access JWT claims 构造，注入到 request extensions）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub login_id: LoginId,
    /// 当前请求的设备类型
    pub device: DeviceType,
    /// 用户档案（按用户类型区分字段）
    pub profile: UserProfile,
}

/// Redis 设备登录信息（存储在 auth:device:{login_id}:{device}）
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

/// validate_token 返回的验证结果（从 Access JWT claims 提取）
#[derive(Debug, Clone)]
pub struct ValidatedAccess {
    pub login_id: LoginId,
    pub device: DeviceType,
    pub user_name: String,
    pub nick_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}
