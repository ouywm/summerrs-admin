use serde::{Deserialize, Serialize};

/// 完整的登录身份标识（单用户模式下仅包含 user_id）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoginId {
    pub user_id: i64,
}

impl LoginId {
    /// 通过 user_id 构造登录身份
    pub const fn new(user_id: i64) -> Self {
        Self { user_id }
    }

    /// session 的 Redis key（如 `auth:session:123`）。
    pub fn session_key(&self) -> String {
        format!("auth:session:{}", self.user_id)
    }

    /// 编码为字符串形式（如 `123`）。
    pub fn encode(&self) -> String {
        self.user_id.to_string()
    }

    /// 从字符串解码（如 `123`）
    ///
    /// 为了避免旧多用户格式误入，这里会拒绝带 `:` 的历史值。
    pub fn decode(s: &str) -> Option<Self> {
        if s.is_empty() || s.contains(':') {
            return None;
        }

        s.parse().ok().map(Self::new)
    }
}

impl std::fmt::Display for LoginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_id)
    }
}

/// 设备类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeviceType {
    #[default]
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
        let id = LoginId::new(42);
        let encoded = id.encode();
        assert_eq!(encoded, "42");
        let decoded = LoginId::decode(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn login_id_decode_rejects_legacy_type_prefix() {
        assert!(LoginId::decode("admin:1").is_none());
        assert!(LoginId::decode("biz:2").is_none());
        assert!(LoginId::decode("user:3").is_none());
    }

    #[test]
    fn login_id_decode_invalid() {
        assert!(LoginId::decode("").is_none());
        assert!(LoginId::decode("abc").is_none());
        assert!(LoginId::decode("1:2").is_none());
    }

    #[test]
    fn session_key_format() {
        let id = LoginId::new(100);
        assert_eq!(id.session_key(), "auth:session:100");
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
}
