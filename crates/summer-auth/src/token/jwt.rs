use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::config::{AuthConfig, JwtAlgorithm};
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

/// JWT 编解码器（支持 HMAC / RSA / ECDSA / EdDSA 算法）
#[derive(Clone)]
pub struct JwtHandler {
    algorithm: Algorithm,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtHandler {
    /// 用 HMAC 密钥创建 JwtHandler（兼容旧接口，默认 HS256）
    pub fn new(secret: &str) -> Self {
        Self {
            algorithm: Algorithm::HS256,
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    /// 根据 AuthConfig 创建 JwtHandler（支持所有算法 + 密钥文件）
    pub fn from_config(config: &AuthConfig) -> Self {
        let algorithm = config.jwt_algorithm.into_algorithm();

        if config.jwt_algorithm.is_symmetric() {
            // HMAC 系列：使用 jwt_secret
            let secret = config
                .jwt_secret
                .as_deref()
                .expect("jwt_secret must be set when using HMAC algorithm (HS256/HS384/HS512)");

            Self {
                algorithm,
                encoding_key: EncodingKey::from_secret(secret.as_bytes()),
                decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            }
        } else {
            // 非对称算法：读取 PEM 密钥文件
            let private_key_path = config.jwt_private_key.as_deref().unwrap_or_else(|| {
                panic!(
                    "jwt_private_key must be set when using asymmetric algorithm ({:?})",
                    config.jwt_algorithm
                )
            });
            let public_key_path = config.jwt_public_key.as_deref().unwrap_or_else(|| {
                panic!(
                    "jwt_public_key must be set when using asymmetric algorithm ({:?})",
                    config.jwt_algorithm
                )
            });

            let private_pem = std::fs::read(private_key_path).unwrap_or_else(|e| {
                panic!("Failed to read JWT private key file '{private_key_path}': {e}")
            });
            let public_pem = std::fs::read(public_key_path).unwrap_or_else(|e| {
                panic!("Failed to read JWT public key file '{public_key_path}': {e}")
            });

            let (encoding_key, decoding_key) = match config.jwt_algorithm {
                JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 => {
                    let ek = EncodingKey::from_rsa_pem(&private_pem)
                        .expect("Invalid RSA private key PEM format");
                    let dk = DecodingKey::from_rsa_pem(&public_pem)
                        .expect("Invalid RSA public key PEM format");
                    (ek, dk)
                }
                JwtAlgorithm::ES256 | JwtAlgorithm::ES384 => {
                    let ek = EncodingKey::from_ec_pem(&private_pem)
                        .expect("Invalid EC private key PEM format");
                    let dk = DecodingKey::from_ec_pem(&public_pem)
                        .expect("Invalid EC public key PEM format");
                    (ek, dk)
                }
                JwtAlgorithm::EdDSA => {
                    let ek = EncodingKey::from_ed_pem(&private_pem)
                        .expect("Invalid Ed25519 private key PEM format");
                    let dk = DecodingKey::from_ed_pem(&public_pem)
                        .expect("Invalid Ed25519 public key PEM format");
                    (ek, dk)
                }
                _ => unreachable!("symmetric algorithms handled above"),
            };

            Self {
                algorithm,
                encoding_key,
                decoding_key,
            }
        }
    }

    /// 编码生成 JWT token
    ///
    /// 返回 (token_string, claims)
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

impl JwtAlgorithm {
    /// 转换为 jsonwebtoken 的 Algorithm 枚举
    pub fn into_algorithm(self) -> Algorithm {
        match self {
            Self::HS256 => Algorithm::HS256,
            Self::HS384 => Algorithm::HS384,
            Self::HS512 => Algorithm::HS512,
            Self::RS256 => Algorithm::RS256,
            Self::RS384 => Algorithm::RS384,
            Self::RS512 => Algorithm::RS512,
            Self::ES256 => Algorithm::ES256,
            Self::ES384 => Algorithm::ES384,
            Self::EdDSA => Algorithm::EdDSA,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_type::LoginId;

    const TEST_SECRET: &str = "test-secret-key-for-jwt-unit-tests";

    #[test]
    fn encode_decode_access_token() {
        let handler = JwtHandler::new(TEST_SECRET);
        let login_id = LoginId::admin(42);

        let (token, claims) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert!(!token.is_empty());
        assert_eq!(claims.sub, "admin:42");
        assert_eq!(claims.typ, TokenType::Access);
        // JTI 是 simple_uuid：32 位 hex
        assert_eq!(claims.jti.len(), 32);
        assert!(claims.jti.chars().all(|c| c.is_ascii_hexdigit()));

        // JWT 格式：3 段 base64 用 . 分隔
        assert_eq!(token.split('.').count(), 3);

        // 解码验证
        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.typ, claims.typ);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn encode_decode_refresh_token() {
        let handler = JwtHandler::new(TEST_SECRET);
        let login_id = LoginId::business(10);

        let (token, claims) = handler.encode(&login_id, TokenType::Refresh, 86400).unwrap();
        assert_eq!(claims.typ, TokenType::Refresh);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.typ, TokenType::Refresh);
        assert_eq!(decoded.sub, "biz:10");
    }

    #[test]
    fn extract_login_id_works() {
        let handler = JwtHandler::new(TEST_SECRET);
        let login_id = LoginId::customer(99);

        let (token, _) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (extracted_id, extracted_claims) = handler.extract_login_id(&token).unwrap();
        assert_eq!(extracted_id, login_id);
        assert_eq!(extracted_claims.typ, TokenType::Access);
    }

    #[test]
    fn invalid_token_rejected() {
        let handler = JwtHandler::new(TEST_SECRET);
        let result = handler.decode("not.a.valid-jwt");
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn wrong_secret_rejected() {
        let handler1 = JwtHandler::new("secret-1");
        let handler2 = JwtHandler::new("secret-2");

        let login_id = LoginId::admin(1);
        let (token, _) = handler1.encode(&login_id, TokenType::Access, 3600).unwrap();

        let result = handler2.decode(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn expired_token_detected() {
        let handler = JwtHandler::new(TEST_SECRET);
        let login_id = LoginId::admin(1);

        // TTL = -120 秒，确保已经过期（超过 jsonwebtoken 默认 leeway）
        let (token, _) = handler.encode(&login_id, TokenType::Access, -120).unwrap();

        let result = handler.decode(&token);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn jti_is_unique() {
        let handler = JwtHandler::new(TEST_SECRET);
        let login_id = LoginId::admin(1);
        let (_, c1) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (_, c2) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert_ne!(c1.jti, c2.jti);
    }

    // ── 多算法测试 ──

    fn hmac_config(algorithm: &str, secret: &str) -> AuthConfig {
        serde_json::from_str(&format!(
            r#"{{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "token_style": "jwt",
                "jwt_algorithm": "{algorithm}",
                "jwt_secret": "{secret}"
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn hs384_encode_decode() {
        let config = hmac_config("HS384", TEST_SECRET);
        let handler = JwtHandler::from_config(&config);
        let login_id = LoginId::admin(1);

        let (token, claims) = handler.encode(&login_id, TokenType::Access, 3600).unwrap();
        assert_eq!(token.split('.').count(), 3);

        let decoded = handler.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn hs512_encode_decode() {
        let config = hmac_config("HS512", TEST_SECRET);
        let handler = JwtHandler::from_config(&config);
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

        let h256 = JwtHandler::from_config(&hmac_config("HS256", TEST_SECRET));
        let h384 = JwtHandler::from_config(&hmac_config("HS384", TEST_SECRET));
        let h512 = JwtHandler::from_config(&hmac_config("HS512", TEST_SECRET));

        let (t256, _) = h256.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (t384, _) = h384.encode(&login_id, TokenType::Access, 3600).unwrap();
        let (t512, _) = h512.encode(&login_id, TokenType::Access, 3600).unwrap();

        // 不同算法的签名部分不同（header 也不同）
        assert_ne!(t256.split('.').nth(0), t384.split('.').nth(0));
        assert_ne!(t384.split('.').nth(0), t512.split('.').nth(0));
    }

    #[test]
    fn cross_algorithm_verification_fails() {
        let login_id = LoginId::admin(1);

        let h256 = JwtHandler::from_config(&hmac_config("HS256", TEST_SECRET));
        let h384 = JwtHandler::from_config(&hmac_config("HS384", TEST_SECRET));

        // 用 HS256 签发的 token 不能用 HS384 验证
        let (token, _) = h256.encode(&login_id, TokenType::Access, 3600).unwrap();
        let result = h384.decode(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn jwt_algorithm_is_symmetric() {
        assert!(JwtAlgorithm::HS256.is_symmetric());
        assert!(JwtAlgorithm::HS384.is_symmetric());
        assert!(JwtAlgorithm::HS512.is_symmetric());
        assert!(!JwtAlgorithm::RS256.is_symmetric());
        assert!(!JwtAlgorithm::ES256.is_symmetric());
        assert!(!JwtAlgorithm::EdDSA.is_symmetric());
    }
}
