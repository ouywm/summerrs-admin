use serde::{Deserialize, Serialize};

/// 用户类型枚举 — 通过不同的 prefix 隔离 Redis 键空间
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserType {
    /// 系统管理员（后台）
    Admin,
    /// B 端用户（企业/商家）
    Business,
    /// C 端用户（普通用户/消费者）
    Customer,
}

impl UserType {
    /// 所有用户类型（新增类型时必须在此添加，其他 exhaustive match 会编译报错提醒）
    pub const fn all() -> &'static [UserType] {
        &[UserType::Admin, UserType::Business, UserType::Customer]
    }

    /// Redis key 前缀，用于隔离不同类型用户的会话空间
    pub fn prefix(&self) -> &'static str {
        match self {
            UserType::Admin => "auth:admin",
            UserType::Business => "auth:biz",
            UserType::Customer => "auth:user",
        }
    }

    /// 该用户类型对应的登录 API 路径
    pub fn login_path(&self) -> &'static str {
        match self {
            UserType::Admin => "/auth/login",
            UserType::Business => "/auth/biz/login",
            UserType::Customer => "/auth/customer/login",
        }
    }

    /// 收集所有用户类型的登录路径（用于 PathAuthBuilder exclude）
    pub fn all_login_paths() -> Vec<&'static str> {
        Self::all().iter().map(|t| t.login_path()).collect()
    }

    /// 使用此类型和指定 user_id 构造 LoginId
    pub fn login_id(&self, user_id: i64) -> LoginId {
        LoginId {
            user_type: *self,
            user_id,
        }
    }
}

impl std::fmt::Display for UserType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserType::Admin => write!(f, "admin"),
            UserType::Business => write!(f, "biz"),
            UserType::Customer => write!(f, "user"),
        }
    }
}

/// 完整的登录身份标识（用户类型 + 用户ID）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoginId {
    pub user_type: UserType,
    pub user_id: i64,
}

impl LoginId {
    pub fn admin(user_id: i64) -> Self {
        Self {
            user_type: UserType::Admin,
            user_id,
        }
    }

    pub fn business(user_id: i64) -> Self {
        Self {
            user_type: UserType::Business,
            user_id,
        }
    }

    pub fn customer(user_id: i64) -> Self {
        Self {
            user_type: UserType::Customer,
            user_id,
        }
    }

    /// session 的 Redis key（如 "auth:admin:session:123"）
    pub fn session_key(&self) -> String {
        format!("{}:session:{}", self.user_type.prefix(), self.user_id)
    }

    /// 编码为字符串形式（如 "admin:123"）
    pub fn encode(&self) -> String {
        format!("{}:{}", self.user_type, self.user_id)
    }

    /// 从字符串解码（如 "admin:123"）
    pub fn decode(s: &str) -> Option<Self> {
        let (type_str, id_str) = s.split_once(':')?;
        let user_type = match type_str {
            "admin" => UserType::Admin,
            "biz" => UserType::Business,
            "user" => UserType::Customer,
            _ => return None,
        };
        let user_id = id_str.parse().ok()?;
        Some(Self { user_type, user_id })
    }
}

impl std::fmt::Display for LoginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.user_type, self.user_id)
    }
}

/// 设备类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    Web,
    Android,
    IOS,
    MiniProgram,
    Tablet,
    Desktop,
    Unknown(String),
}

impl DeviceType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Web => "web",
            Self::Android => "android",
            Self::IOS => "ios",
            Self::MiniProgram => "mini_program",
            Self::Tablet => "tablet",
            Self::Desktop => "desktop",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

impl Default for DeviceType {
    fn default() -> Self {
        Self::Web
    }
}

impl From<&str> for DeviceType {
    fn from(s: &str) -> Self {
        match s {
            "web" => Self::Web,
            "android" => Self::Android,
            "ios" => Self::IOS,
            "mini_program" => Self::MiniProgram,
            "tablet" => Self::Tablet,
            "desktop" => Self::Desktop,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_id_encode_decode() {
        let id = LoginId::admin(42);
        let encoded = id.encode();
        assert_eq!(encoded, "admin:42");
        let decoded = LoginId::decode(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn login_id_decode_all_types() {
        assert_eq!(LoginId::decode("admin:1").unwrap(), LoginId::admin(1));
        assert_eq!(LoginId::decode("biz:2").unwrap(), LoginId::business(2));
        assert_eq!(LoginId::decode("user:3").unwrap(), LoginId::customer(3));
    }

    #[test]
    fn login_id_decode_invalid() {
        assert!(LoginId::decode("").is_none());
        assert!(LoginId::decode("admin").is_none());
        assert!(LoginId::decode("unknown:1").is_none());
        assert!(LoginId::decode("admin:abc").is_none());
    }

    #[test]
    fn session_key_format() {
        let id = LoginId::admin(100);
        assert_eq!(id.session_key(), "auth:admin:session:100");

        let id = LoginId::business(200);
        assert_eq!(id.session_key(), "auth:biz:session:200");

        let id = LoginId::customer(300);
        assert_eq!(id.session_key(), "auth:user:session:300");
    }

    #[test]
    fn device_type_display() {
        assert_eq!(DeviceType::Web.to_string(), "web");
        assert_eq!(DeviceType::Android.to_string(), "android");
        assert_eq!(DeviceType::IOS.to_string(), "ios");
        assert_eq!(
            DeviceType::Unknown("custom".to_string()).to_string(),
            "custom"
        );
    }

    #[test]
    fn user_type_prefix() {
        assert_eq!(UserType::Admin.prefix(), "auth:admin");
        assert_eq!(UserType::Business.prefix(), "auth:biz");
        assert_eq!(UserType::Customer.prefix(), "auth:user");
    }

    #[test]
    fn all_covers_every_variant() {
        // 确保 all() 中的数量等于 enum 变体数
        // 如果有人加了新变体但忘了加到 all()，
        // 虽然编译不报错，但其他 exhaustive match 会报错
        assert_eq!(UserType::all().len(), 3);
    }

    #[test]
    fn login_path_all_types() {
        assert_eq!(UserType::Admin.login_path(), "/auth/login");
        assert_eq!(UserType::Business.login_path(), "/auth/biz/login");
        assert_eq!(UserType::Customer.login_path(), "/auth/customer/login");
    }

    #[test]
    fn all_login_paths_complete() {
        let paths = UserType::all_login_paths();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"/auth/login"));
        assert!(paths.contains(&"/auth/biz/login"));
        assert!(paths.contains(&"/auth/customer/login"));
    }

    #[test]
    fn login_id_from_user_type() {
        let lid = UserType::Admin.login_id(42);
        assert_eq!(lid, LoginId::admin(42));

        let lid = UserType::Business.login_id(10);
        assert_eq!(lid, LoginId::business(10));

        let lid = UserType::Customer.login_id(99);
        assert_eq!(lid, LoginId::customer(99));
    }
}
