use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AuthError, AuthResult};
use crate::session::model::UserProfile;
use crate::user_type::{DeviceType, LoginId};

/// JWT token 类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

/// Access JWT Claims — 自包含用户信息，中间件无需查 Redis Session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// 签发者标识
    pub iss: String,
    /// 受众标识
    pub aud: String,
    /// Subject — 编码后的 LoginId（如 "123"）
    pub sub: String,
    /// Token 类型 — 固定为 Access
    pub typ: TokenType,
    /// 签发时间（Unix 时间戳）
    pub iat: i64,
    /// 过期时间（Unix 时间戳）
    pub exp: i64,
    /// 设备类型
    pub dev: String,
    /// 用户名
    pub user_name: String,
    /// 昵称
    pub nick_name: String,
    /// 角色列表
    pub roles: Vec<String>,
    /// 权限列表（无 bitmap 时使用）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    /// 权限位图（Base64 编码，有 PermissionMap 时使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
}

/// Refresh JWT Claims — 仅包含身份标识和 Refresh UUID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    /// Subject — 编码后的 LoginId（如 "123"）
    pub sub: String,
    /// Token 类型 — 固定为 Refresh
    pub typ: TokenType,
    /// 签发时间（Unix 时间戳）
    pub iat: i64,
    /// 过期时间（Unix 时间戳）
    pub exp: i64,
    /// Refresh ID — UUID，对应 Redis 中 auth:refresh:{rid}
    pub rid: String,
}

/// JWT 编解码器（纯加密原语包装，不依赖任何配置类型）
#[derive(Clone)]
pub struct JwtHandler {
    algorithm: Algorithm,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    audience: String,
}

impl JwtHandler {
    /// 从加密原语直接构造
    pub fn new(
        algorithm: Algorithm,
        encoding_key: EncodingKey,
        decoding_key: DecodingKey,
        issuer: String,
        audience: String,
    ) -> Self {
        Self {
            algorithm,
            encoding_key,
            decoding_key,
            issuer,
            audience,
        }
    }

    /// HMAC 快捷构造（默认 HS256，用于测试和简单场景）
    #[cfg(test)]
    pub fn hmac(secret: &str) -> Self {
        Self::new(
            Algorithm::HS256,
            EncodingKey::from_secret(secret.as_bytes()),
            DecodingKey::from_secret(secret.as_bytes()),
            "test".to_string(),
            "test".to_string(),
        )
    }

    /// 编码 Access JWT — 自包含用户信息
    pub fn encode_access(
        &self,
        login_id: &LoginId,
        device: &DeviceType,
        profile: &UserProfile,
        pb: Option<&str>,
        ttl_seconds: i64,
    ) -> AuthResult<(String, AccessClaims)> {
        let now = chrono::Local::now().timestamp();
        let claims = AccessClaims {
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            sub: login_id.encode(),
            typ: TokenType::Access,
            iat: now,
            exp: now + ttl_seconds,
            dev: device.as_str().to_string(),
            user_name: profile.user_name().to_string(),
            nick_name: profile.nick_name().to_string(),
            roles: profile.roles().to_vec(),
            permissions: if pb.is_some() {
                vec![]
            } else {
                profile.permissions().to_vec()
            },
            pb: pb.map(|s| s.to_string()),
        };

        let token = jsonwebtoken::encode(&Header::new(self.algorithm), &claims, &self.encoding_key)
            .map_err(|e| AuthError::Internal(format!("JWT encode error: {e}")))?;

        Ok((token, claims))
    }

    /// 编码 Refresh JWT — 包裹 UUID 用于轮转
    pub fn encode_refresh(
        &self,
        login_id: &LoginId,
        ttl_seconds: i64,
    ) -> AuthResult<(String, RefreshClaims)> {
        let now = chrono::Local::now().timestamp();
        let claims = RefreshClaims {
            sub: login_id.encode(),
            typ: TokenType::Refresh,
            iat: now,
            exp: now + ttl_seconds,
            rid: Uuid::new_v4().to_string(),
        };

        let token = jsonwebtoken::encode(&Header::new(self.algorithm), &claims, &self.encoding_key)
            .map_err(|e| AuthError::Internal(format!("JWT encode error: {e}")))?;

        Ok((token, claims))
    }

    /// 解码 Access JWT — 验证签名 + 过期 + 类型 + iss/aud
    pub fn decode_access(&self, token: &str) -> AuthResult<AccessClaims> {
        let mut validation = Validation::new(self.algorithm);
        validation.validate_exp = true;
        validation.leeway = 0;
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);

        let token_data =
            jsonwebtoken::decode::<AccessClaims>(token, &self.decoding_key, &validation).map_err(
                |e| match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                    _ => AuthError::InvalidToken,
                },
            )?;

        if token_data.claims.typ != TokenType::Access {
            return Err(AuthError::InvalidToken);
        }

        Ok(token_data.claims)
    }

    /// 解码 Refresh JWT — 验证签名 + 过期 + 类型
    pub fn decode_refresh(&self, token: &str) -> AuthResult<RefreshClaims> {
        let mut validation = Validation::new(self.algorithm);
        validation.validate_exp = true;
        validation.leeway = 0;

        let token_data = jsonwebtoken::decode::<RefreshClaims>(
            token,
            &self.decoding_key,
            &validation,
        )
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::RefreshTokenExpired,
            _ => AuthError::InvalidRefreshToken,
        })?;

        if token_data.claims.typ != TokenType::Refresh {
            return Err(AuthError::InvalidRefreshToken);
        }

        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_type::LoginId;

    const TEST_SECRET: &str = "test-secret-key-for-jwt-unit-tests";

    fn test_profile() -> UserProfile {
        UserProfile {
            user_name: "admin".to_string(),
            nick_name: "管理员".to_string(),
            roles: vec!["admin".to_string()],
            permissions: vec![
                "system:user:list".to_string(),
                "system:user:add".to_string(),
            ],
        }
    }

    #[test]
    fn encode_decode_access_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(42);
        let device = DeviceType::Web;
        let profile = test_profile();

        let (token, claims) = handler
            .encode_access(&login_id, &device, &profile, None, 3600)
            .unwrap();

        assert!(!token.is_empty());
        assert_eq!(claims.sub, "42");
        assert_eq!(claims.typ, TokenType::Access);
        assert_eq!(claims.dev, "web");
        assert_eq!(claims.user_name, "admin");
        assert_eq!(claims.nick_name, "管理员");
        assert_eq!(claims.iss, "test");
        assert_eq!(claims.aud, "test");
        assert_eq!(claims.roles, vec!["admin"]);
        assert_eq!(claims.permissions.len(), 2);
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode_access(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.typ, claims.typ);
        assert_eq!(decoded.dev, claims.dev);
        assert_eq!(decoded.user_name, claims.user_name);
        assert_eq!(decoded.roles, claims.roles);
        assert_eq!(decoded.permissions, claims.permissions);
    }

    #[test]
    fn encode_decode_refresh_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(10);

        let (token, claims) = handler.encode_refresh(&login_id, 86400).unwrap();
        assert_eq!(claims.typ, TokenType::Refresh);
        assert!(!claims.rid.is_empty());
        assert!(claims.rid.contains('-')); // UUID 格式

        let decoded = handler.decode_refresh(&token).unwrap();
        assert_eq!(decoded.typ, TokenType::Refresh);
        assert_eq!(decoded.sub, "10");
        assert_eq!(decoded.rid, claims.rid);
    }

    #[test]
    fn access_token_cannot_decode_as_refresh() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let (token, _) = handler
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();

        // Access token 解码为 Refresh 应该失败
        let result = handler.decode_refresh(&token);
        assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
    }

    #[test]
    fn refresh_token_cannot_decode_as_access() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(1);

        let (token, _) = handler.encode_refresh(&login_id, 86400).unwrap();

        // Refresh token 解码为 Access 应该失败（claims 结构不同 + typ 不匹配）
        let result = handler.decode_access(&token);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_token_rejected() {
        let handler = JwtHandler::hmac(TEST_SECRET);

        let result = handler.decode_access("not.a.valid-jwt");
        assert!(matches!(result, Err(AuthError::InvalidToken)));

        let result = handler.decode_refresh("not.a.valid-jwt");
        assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
    }

    #[test]
    fn wrong_secret_rejected() {
        let handler1 = JwtHandler::hmac("secret-1");
        let handler2 = JwtHandler::hmac("secret-2");

        let login_id = LoginId::new(1);
        let profile = test_profile();
        let (token, _) = handler1
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();

        let result = handler2.decode_access(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn expired_access_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let (token, _) = handler
            .encode_access(&login_id, &DeviceType::Web, &profile, None, -120)
            .unwrap();

        let result = handler.decode_access(&token);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn expired_refresh_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(1);

        let (token, _) = handler.encode_refresh(&login_id, -120).unwrap();

        let result = handler.decode_refresh(&token);
        assert!(matches!(result, Err(AuthError::RefreshTokenExpired)));
    }

    #[test]
    fn rid_is_unique() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::new(1);

        let (_, c1) = handler.encode_refresh(&login_id, 86400).unwrap();
        let (_, c2) = handler.encode_refresh(&login_id, 86400).unwrap();
        assert_ne!(c1.rid, c2.rid);
    }

    // ── 多算法测试 ──

    fn hmac_handler(algorithm: Algorithm, secret: &str) -> JwtHandler {
        JwtHandler::new(
            algorithm,
            EncodingKey::from_secret(secret.as_bytes()),
            DecodingKey::from_secret(secret.as_bytes()),
            "test".to_string(),
            "test".to_string(),
        )
    }

    #[test]
    fn hs384_encode_decode() {
        let handler = hmac_handler(Algorithm::HS384, TEST_SECRET);
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let (token, claims) = handler
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode_access(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.user_name, claims.user_name);
    }

    #[test]
    fn hs512_encode_decode() {
        let handler = hmac_handler(Algorithm::HS512, TEST_SECRET);
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let (token, claims) = handler
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode_access(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
    }

    #[test]
    fn different_algorithms_produce_different_tokens() {
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let h256 = hmac_handler(Algorithm::HS256, TEST_SECRET);
        let h384 = hmac_handler(Algorithm::HS384, TEST_SECRET);
        let h512 = hmac_handler(Algorithm::HS512, TEST_SECRET);

        let (t256, _) = h256
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();
        let (t384, _) = h384
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();
        let (t512, _) = h512
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();

        assert_ne!(t256.split('.').next(), t384.split('.').next());
        assert_ne!(t384.split('.').next(), t512.split('.').next());
    }

    #[test]
    fn cross_algorithm_verification_fails() {
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let h256 = hmac_handler(Algorithm::HS256, TEST_SECRET);
        let h384 = hmac_handler(Algorithm::HS384, TEST_SECRET);

        let (token, _) = h256
            .encode_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();
        let result = h384.decode_access(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }
}
