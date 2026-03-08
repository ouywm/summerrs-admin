use uuid::Uuid;

use crate::config::{AuthConfig, TokenStyle};
use crate::error::AuthResult;
use crate::token::jwt::{JwtHandler, TokenType};
use crate::user_type::LoginId;

/// 生成的 Token 结果（统一 UUID 和 JWT 两种模式的输出）
#[derive(Debug, Clone)]
pub struct GeneratedToken {
    /// token 字符串（UUID 或 JWT）
    pub token: String,
    /// JWT 模式下的 JTI（用于黑名单），UUID 模式为 None
    pub jti: Option<String>,
}

/// Token 对（access + refresh）
#[derive(Debug, Clone)]
pub struct GeneratedTokenPair {
    pub access: GeneratedToken,
    pub refresh: GeneratedToken,
}

/// Token 生成器（根据配置自动选择 UUID 或 JWT 模式）
#[derive(Clone)]
pub struct TokenGenerator {
    style: TokenStyle,
    jwt_handler: Option<JwtHandler>,
}

impl TokenGenerator {
    /// 根据 AuthConfig 创建 TokenGenerator
    pub fn new(config: &AuthConfig) -> Self {
        let jwt_handler = if matches!(config.token_style, TokenStyle::Jwt) {
            Some(JwtHandler::from_config(config))
        } else {
            None
        };

        Self {
            style: config.token_style,
            jwt_handler,
        }
    }

    /// 是否为 JWT 模式
    pub fn is_jwt(&self) -> bool {
        self.jwt_handler.is_some()
    }

    /// 获取 JwtHandler 引用（JWT 模式下必定 Some）
    pub fn jwt(&self) -> &JwtHandler {
        self.jwt_handler
            .as_ref()
            .expect("jwt_handler must be Some in JWT mode")
    }

    /// 生成 access + refresh token 对
    pub fn generate_pair(
        &self,
        login_id: &LoginId,
        access_ttl: i64,
        refresh_ttl: i64,
    ) -> AuthResult<GeneratedTokenPair> {
        match self.style {
            TokenStyle::Uuid => Ok(GeneratedTokenPair {
                access: GeneratedToken {
                    token: Self::uuid(),
                    jti: None,
                },
                refresh: GeneratedToken {
                    token: Self::uuid(),
                    jti: None,
                },
            }),
            TokenStyle::Jwt => {
                let jwt = self.jwt();
                let (access_token, access_claims) =
                    jwt.encode(login_id, TokenType::Access, access_ttl)?;
                let (refresh_token, refresh_claims) =
                    jwt.encode(login_id, TokenType::Refresh, refresh_ttl)?;
                Ok(GeneratedTokenPair {
                    access: GeneratedToken {
                        token: access_token,
                        jti: Some(access_claims.jti),
                    },
                    refresh: GeneratedToken {
                        token: refresh_token,
                        jti: Some(refresh_claims.jti),
                    },
                })
            }
        }
    }

    /// 只生成 access token（用于 refresh 流程）
    pub fn generate_access(
        &self,
        login_id: &LoginId,
        access_ttl: i64,
    ) -> AuthResult<GeneratedToken> {
        match self.style {
            TokenStyle::Uuid => Ok(GeneratedToken {
                token: Self::uuid(),
                jti: None,
            }),
            TokenStyle::Jwt => {
                let jwt = self.jwt();
                let (token, claims) = jwt.encode(login_id, TokenType::Access, access_ttl)?;
                Ok(GeneratedToken {
                    token,
                    jti: Some(claims.jti),
                })
            }
        }
    }

    /// 标准 UUID v4
    pub fn uuid() -> String {
        Uuid::new_v4().to_string()
    }

    /// 无连字符 UUID（用于 JTI 等内部标识）
    pub fn simple_uuid() -> String {
        Uuid::new_v4().simple().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "token_style": "uuid"
            }"#,
        )
        .unwrap()
    }

    fn jwt_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "token_style": "jwt",
                "jwt_secret": "test-secret-key"
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn uuid_mode_generate_pair() {
        let generator = TokenGenerator::new(&uuid_config());
        assert!(!generator.is_jwt());

        let pair = generator
            .generate_pair(&LoginId::admin(1), 3600, 86400)
            .unwrap();
        assert_eq!(pair.access.token.len(), 36);
        assert!(pair.access.token.contains('-'));
        assert!(pair.access.jti.is_none());
        assert!(pair.refresh.jti.is_none());
    }

    #[test]
    fn jwt_mode_generate_pair() {
        let generator = TokenGenerator::new(&jwt_config());
        assert!(generator.is_jwt());

        let pair = generator
            .generate_pair(&LoginId::admin(1), 3600, 86400)
            .unwrap();
        // JWT 格式
        assert_eq!(pair.access.token.split('.').count(), 3);
        assert_eq!(pair.refresh.token.split('.').count(), 3);
        // 有 JTI
        assert!(pair.access.jti.is_some());
        assert!(pair.refresh.jti.is_some());
        // JTI 不同
        assert_ne!(pair.access.jti, pair.refresh.jti);
    }

    #[test]
    fn uuid_mode_generate_access() {
        let generator = TokenGenerator::new(&uuid_config());
        let t = generator.generate_access(&LoginId::admin(1), 3600).unwrap();
        assert_eq!(t.token.len(), 36);
        assert!(t.jti.is_none());
    }

    #[test]
    fn jwt_mode_generate_access() {
        let generator = TokenGenerator::new(&jwt_config());
        let t = generator.generate_access(&LoginId::admin(1), 3600).unwrap();
        assert_eq!(t.token.split('.').count(), 3);
        assert!(t.jti.is_some());
    }

    #[test]
    fn tokens_are_unique() {
        let a = TokenGenerator::uuid();
        let b = TokenGenerator::uuid();
        assert_ne!(a, b);
    }
}
