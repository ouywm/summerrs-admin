use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{AuthError, AuthResult};
use crate::token::generator::TokenGenerator;
use crate::user_type::LoginId;

/// JWT token 类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

/// JWT Claims 载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject — 编码后的 LoginId（如 "admin:123"）
    pub sub: String,
    /// Token 类型（access / refresh）
    pub typ: TokenType,
    /// 签发时间（Unix 时间戳）
    pub iat: i64,
    /// 过期时间（Unix 时间戳）
    pub exp: i64,
    /// JWT ID — 用于黑名单 key 后缀
    pub jti: String,
}

/// JWT 编解码器（纯加密原语包装，不依赖任何配置类型）
#[derive(Clone)]
pub struct JwtHandler {
    algorithm: Algorithm,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtHandler {
    /// 从加密原语直接构造
    pub fn new(algorithm: Algorithm, encoding_key: EncodingKey, decoding_key: DecodingKey) -> Self {
        Self {
            algorithm,
            encoding_key,
            decoding_key,
        }
    }

    /// HMAC 快捷构造（默认 HS256，用于测试和简单场景）
    #[cfg(test)]
    pub fn hmac(secret: &str) -> Self {
        Self::new(
            Algorithm::HS256,
            EncodingKey::from_secret(secret.as_bytes()),
            DecodingKey::from_secret(secret.as_bytes()),
        )
    }

    /// 编码生成 JWT token，返回 (token_string, claims)
    pub fn encode(
        &self,
        login_id: &LoginId,
        token_type: TokenType,
        ttl_seconds: i64,
    ) -> AuthResult<(String, JwtClaims)> {
        let now = chrono::Local::now().timestamp();
        let claims = JwtClaims {
            sub: login_id.encode(),
            typ: token_type,
            iat: now,
            exp: now + ttl_seconds,
            jti: TokenGenerator::simple_uuid(),
        };

        let token =
            jsonwebtoken::encode(&Header::new(self.algorithm), &claims, &self.encoding_key)
                .map_err(|e| AuthError::Internal(format!("JWT encode error: {e}")))?;

        Ok((token, claims))
    }

    /// 解码验证 JWT token（验证签名 + 过期）
    pub fn decode(&self, token: &str) -> AuthResult<JwtClaims> {
        let mut validation = Validation::new(self.algorithm);
        validation.validate_exp = true;
        validation.leeway = 0;

        let token_data =
            jsonwebtoken::decode::<JwtClaims>(token, &self.decoding_key, &validation).map_err(
                |e| match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                    _ => AuthError::InvalidToken,
                },
            )?;

        Ok(token_data.claims)
    }

    /// 从 token 中提取 LoginId 和 Claims
    pub fn extract_login_id(&self, token: &str) -> AuthResult<(LoginId, JwtClaims)> {
        let claims = self.decode(token)?;
        let login_id = LoginId::decode(&claims.sub).ok_or(AuthError::InvalidToken)?;
        Ok((login_id, claims))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_type::LoginId;

    const TEST_SECRET: &str = "test-secret-key-for-jwt-unit-tests";

    #[test]
    fn encode_decode_access_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::admin(42);

        let (token, claims) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert!(!token.is_empty());
        assert_eq!(claims.sub, "admin:42");
        assert_eq!(claims.typ, TokenType::Access);
        assert_eq!(claims.jti.len(), 32);
        assert!(claims.jti.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.typ, claims.typ);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn encode_decode_refresh_token() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::business(10);

        let (token, claims) = handler.encode(&login_id, TokenType::Refresh, 86400).unwrap();
        assert_eq!(claims.typ, TokenType::Refresh);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.typ, TokenType::Refresh);
        assert_eq!(decoded.sub, "biz:10");
    }

    #[test]
    fn extract_login_id_works() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::customer(99);

        let (token, _) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (extracted_id, extracted_claims) = handler.extract_login_id(&token).unwrap();
        assert_eq!(extracted_id, login_id);
        assert_eq!(extracted_claims.typ, TokenType::Access);
    }

    #[test]
    fn invalid_token_rejected() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let result = handler.decode("not.a.valid-jwt");
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn wrong_secret_rejected() {
        let handler1 = JwtHandler::hmac("secret-1");
        let handler2 = JwtHandler::hmac("secret-2");

        let login_id = LoginId::admin(1);
        let (token, _) = handler1.encode(&login_id, TokenType::Access, 3600).unwrap();

        let result = handler2.decode(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn expired_token_detected() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::admin(1);
        let (token, _) = handler.encode(&login_id, TokenType::Access, -120).unwrap();

        let result = handler.decode(&token);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn jti_is_unique() {
        let handler = JwtHandler::hmac(TEST_SECRET);
        let login_id = LoginId::admin(1);
        let (_, c1) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (_, c2) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert_ne!(c1.jti, c2.jti);
    }

    // ── 多算法测试 ──

    fn hmac_handler(algorithm: Algorithm, secret: &str) -> JwtHandler {
        JwtHandler::new(
            algorithm,
            EncodingKey::from_secret(secret.as_bytes()),
            DecodingKey::from_secret(secret.as_bytes()),
        )
    }

    #[test]
    fn hs384_encode_decode() {
        let handler = hmac_handler(Algorithm::HS384, TEST_SECRET);
        let login_id = LoginId::admin(1);

        let (token, claims) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn hs512_encode_decode() {
        let handler = hmac_handler(Algorithm::HS512, TEST_SECRET);
        let login_id = LoginId::admin(1);

        let (token, claims) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn different_algorithms_produce_different_tokens() {
        let login_id = LoginId::admin(1);

        let h256 = hmac_handler(Algorithm::HS256, TEST_SECRET);
        let h384 = hmac_handler(Algorithm::HS384, TEST_SECRET);
        let h512 = hmac_handler(Algorithm::HS512, TEST_SECRET);

        let (t256, _) = h256.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (t384, _) = h384.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (t512, _) = h512.encode(&login_id, TokenType::Access, 3600).unwrap();

        assert_ne!(t256.split('.').nth(0), t384.split('.').nth(0));
        assert_ne!(t384.split('.').nth(0), t512.split('.').nth(0));
    }

    #[test]
    fn cross_algorithm_verification_fails() {
        let login_id = LoginId::admin(1);

        let h256 = hmac_handler(Algorithm::HS256, TEST_SECRET);
        let h384 = hmac_handler(Algorithm::HS384, TEST_SECRET);

        let (token, _) = h256.encode(&login_id, TokenType::Access, 3600).unwrap();
        let result = h384.decode(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }
}
