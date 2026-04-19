//! 鉴权数据：值本体 + 从 env 懒取的三态。
//!
//! 设计对齐 [genai `AuthData`](https://github.com/jeremychone/rust-genai/blob/main/src/resolver/auth_data.rs)：
//! 三种构造：
//!
//! - [`AuthData::Single`] — 直接给 key（DB 里 channel_account.credentials 解析出来）
//! - [`AuthData::FromEnv`] — 给环境变量名，[`AuthData::resolve`] 时读取
//! - [`AuthData::None`] — 不鉴权（本地 ollama 等）
//!
//! 注意：这里只存 **key 的原始值**，**怎么把它塞 header / query** 是各 Adapter 自己决定
//! （OpenAI 塞 `Authorization: Bearer`、Anthropic 塞 `x-api-key` 等）。

use std::sync::Arc;

use crate::error::AuthResolveError;

/// 鉴权凭证。
#[derive(Debug, Clone, Default)]
pub enum AuthData {
    #[default]
    None,
    Single(Arc<String>),
    FromEnv(Arc<String>),
}

impl AuthData {
    pub fn from_single(value: impl Into<String>) -> Self {
        Self::Single(Arc::new(value.into()))
    }

    pub fn from_env(env_name: impl Into<String>) -> Self {
        Self::FromEnv(Arc::new(env_name.into()))
    }

    /// 真正取 key 字符串；`None` 返回 `Ok(None)`，env 读取失败返 `Err`。
    pub fn resolve(&self) -> Result<Option<String>, AuthResolveError> {
        match self {
            Self::None => Ok(None),
            Self::Single(value) => Ok(Some((**value).clone())),
            Self::FromEnv(name) => match std::env::var(name.as_str()) {
                Ok(value) if !value.is_empty() => Ok(Some(value)),
                Ok(_) => Err(AuthResolveError::EmptyEnv((**name).clone())),
                Err(_) => Err(AuthResolveError::MissingEnv((**name).clone())),
            },
        }
    }

    pub fn is_set(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_resolves_to_value() {
        let auth = AuthData::from_single("sk-xyz");
        assert_eq!(auth.resolve().unwrap().as_deref(), Some("sk-xyz"));
    }

    #[test]
    fn none_resolves_to_none() {
        assert!(AuthData::None.resolve().unwrap().is_none());
    }

    #[test]
    fn from_env_reads_runtime_value() {
        unsafe {
            std::env::set_var("SUMMER_AI_TEST_AUTH_KEY", "from-env");
        }
        let auth = AuthData::from_env("SUMMER_AI_TEST_AUTH_KEY");
        assert_eq!(auth.resolve().unwrap().as_deref(), Some("from-env"));
        unsafe {
            std::env::remove_var("SUMMER_AI_TEST_AUTH_KEY");
        }
    }

    #[test]
    fn from_env_errors_when_missing() {
        let auth = AuthData::from_env("SUMMER_AI_TEST_AUTH_MISSING_KEY_XYZ");
        assert!(matches!(
            auth.resolve(),
            Err(AuthResolveError::MissingEnv(_))
        ));
    }
}
