use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
use uuid::Uuid;

use crate::config::{AuthConfig, JwtAlgorithm};
use crate::error::AuthResult;
use crate::session::model::UserProfile;
use crate::token::jwt::{AccessClaims, JwtHandler, RefreshClaims};
use crate::user_type::{DeviceType, LoginId};

/// Token 生成器（JWT Only）
///
/// 职责边界：
/// - `JwtHandler` 是纯加密原语包装，只管 encode/decode
/// - `TokenGenerator` 是适配层，负责从 AuthConfig 构建 JwtHandler 并提供统一生成接口
#[derive(Clone)]
pub struct TokenGenerator {
    jwt_handler: JwtHandler,
}

impl TokenGenerator {
    /// 根据 AuthConfig 创建 TokenGenerator
    ///
    /// 根据 jwt_algorithm 自动选择密钥类型：
    /// - HMAC 系列（HS256/HS384/HS512）：使用 jwt_secret
    /// - 非对称算法（RS256/ES256/EdDSA 等）：读取 jwt_private_key / jwt_public_key 文件
    pub fn new(config: &AuthConfig) -> Self {
        Self {
            jwt_handler: Self::build_jwt_handler(config),
        }
    }

    /// 获取 JwtHandler 引用
    pub fn jwt(&self) -> &JwtHandler {
        &self.jwt_handler
    }

    /// 生成 Access JWT — 自包含用户信息
    pub fn generate_access(
        &self,
        login_id: &LoginId,
        device: &DeviceType,
        profile: &UserProfile,
        pb: Option<&str>,
        ttl_seconds: i64,
    ) -> AuthResult<(String, AccessClaims)> {
        self.jwt_handler
            .encode_access(login_id, device, profile, pb, ttl_seconds)
    }

    /// 生成 Refresh JWT — 包裹 UUID
    pub fn generate_refresh(
        &self,
        login_id: &LoginId,
        ttl_seconds: i64,
    ) -> AuthResult<(String, RefreshClaims)> {
        self.jwt_handler.encode_refresh(login_id, ttl_seconds)
    }

    /// 标准 UUID v4
    pub fn uuid() -> String {
        Uuid::new_v4().to_string()
    }

    // ── 私有：config → JwtHandler 构建 ──

    fn build_jwt_handler(config: &AuthConfig) -> JwtHandler {
        let algorithm = map_algorithm(config.jwt_algorithm);

        if config.jwt_algorithm.is_symmetric() {
            let secret = config
                .jwt_secret
                .as_deref()
                .expect("jwt_secret must be set when using HMAC algorithm (HS256/HS384/HS512)");

            JwtHandler::new(
                algorithm,
                EncodingKey::from_secret(secret.as_bytes()),
                DecodingKey::from_secret(secret.as_bytes()),
                config.jwt_issuer.clone(),
                config.jwt_audience.clone(),
            )
        } else {
            let (encoding_key, decoding_key) = load_asymmetric_keys(config);
            JwtHandler::new(
                algorithm,
                encoding_key,
                decoding_key,
                config.jwt_issuer.clone(),
                config.jwt_audience.clone(),
            )
        }
    }
}

/// JwtAlgorithm → jsonwebtoken::Algorithm 映射（模块私有）
fn map_algorithm(alg: JwtAlgorithm) -> Algorithm {
    match alg {
        JwtAlgorithm::HS256 => Algorithm::HS256,
        JwtAlgorithm::HS384 => Algorithm::HS384,
        JwtAlgorithm::HS512 => Algorithm::HS512,
        JwtAlgorithm::RS256 => Algorithm::RS256,
        JwtAlgorithm::RS384 => Algorithm::RS384,
        JwtAlgorithm::RS512 => Algorithm::RS512,
        JwtAlgorithm::ES256 => Algorithm::ES256,
        JwtAlgorithm::ES384 => Algorithm::ES384,
        JwtAlgorithm::EdDSA => Algorithm::EdDSA,
    }
}

/// 读取非对称密钥文件并构造 EncodingKey / DecodingKey（模块私有）
fn load_asymmetric_keys(config: &AuthConfig) -> (EncodingKey, DecodingKey) {
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

    let private_pem = std::fs::read(private_key_path)
        .unwrap_or_else(|e| panic!("Failed to read JWT private key '{private_key_path}': {e}"));
    let public_pem = std::fs::read(public_key_path)
        .unwrap_or_else(|e| panic!("Failed to read JWT public key '{public_key_path}': {e}"));

    match config.jwt_algorithm {
        JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 => {
            let ek = EncodingKey::from_rsa_pem(&private_pem).expect("Invalid RSA private key PEM");
            let dk = DecodingKey::from_rsa_pem(&public_pem).expect("Invalid RSA public key PEM");
            (ek, dk)
        }
        JwtAlgorithm::ES256 | JwtAlgorithm::ES384 => {
            let ek = EncodingKey::from_ec_pem(&private_pem).expect("Invalid EC private key PEM");
            let dk = DecodingKey::from_ec_pem(&public_pem).expect("Invalid EC public key PEM");
            (ek, dk)
        }
        JwtAlgorithm::EdDSA => {
            let ek =
                EncodingKey::from_ed_pem(&private_pem).expect("Invalid Ed25519 private key PEM");
            let dk = DecodingKey::from_ed_pem(&public_pem).expect("Invalid Ed25519 public key PEM");
            (ek, dk)
        }
        _ => unreachable!("symmetric algorithms handled by caller"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "jwt_secret": "test-secret-key"
            }"#,
        )
        .unwrap()
    }

    fn test_profile() -> UserProfile {
        UserProfile {
            user_name: "admin".to_string(),
            nick_name: "管理员".to_string(),
            roles: vec!["admin".to_string()],
            permissions: vec!["system:user:list".to_string()],
        }
    }

    #[test]
    fn generate_access_token() {
        let generator = TokenGenerator::new(&jwt_config());
        let login_id = LoginId::new(1);
        let profile = test_profile();

        let (token, claims) = generator
            .generate_access(&login_id, &DeviceType::Web, &profile, None, 3600)
            .unwrap();

        assert_eq!(token.split('.').count(), 3);
        assert_eq!(claims.sub, "1");
        assert_eq!(claims.user_name, "admin");
    }

    #[test]
    fn generate_refresh_token() {
        let generator = TokenGenerator::new(&jwt_config());
        let login_id = LoginId::new(1);

        let (token, claims) = generator.generate_refresh(&login_id, 86400).unwrap();

        assert_eq!(token.split('.').count(), 3);
        assert!(!claims.rid.is_empty());
    }

    #[test]
    fn tokens_are_unique() {
        let a = TokenGenerator::uuid();
        let b = TokenGenerator::uuid();
        assert_ne!(a, b);
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

    #[test]
    fn map_algorithm_covers_all() {
        let variants = [
            JwtAlgorithm::HS256,
            JwtAlgorithm::HS384,
            JwtAlgorithm::HS512,
            JwtAlgorithm::RS256,
            JwtAlgorithm::RS384,
            JwtAlgorithm::RS512,
            JwtAlgorithm::ES256,
            JwtAlgorithm::ES384,
            JwtAlgorithm::EdDSA,
        ];
        for v in variants {
            let _ = map_algorithm(v);
        }
    }
}
