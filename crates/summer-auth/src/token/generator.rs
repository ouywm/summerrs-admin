use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
use uuid::Uuid;

use crate::config::{AuthConfig, JwtAlgorithm, TokenStyle};
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
///
/// 职责边界：
/// - `JwtHandler` 是纯加密原语包装，只管 encode/decode
/// - `TokenGenerator` 是适配层，负责从 AuthConfig 构建 JwtHandler 并提供统一生成接口
#[derive(Clone)]
pub struct TokenGenerator {
    style: TokenStyle,
    jwt_handler: Option<JwtHandler>,
}

impl TokenGenerator {
    /// 根据 AuthConfig 创建 TokenGenerator
    ///
    /// JWT 模式下根据 jwt_algorithm 自动选择密钥类型：
    /// - HMAC 系列（HS256/HS384/HS512）：使用 jwt_secret
    /// - 非对称算法（RS256/ES256/EdDSA 等）：读取 jwt_private_key / jwt_public_key 文件
    pub fn new(config: &AuthConfig) -> Self {
        let jwt_handler = if matches!(config.token_style, TokenStyle::Jwt) {
            Some(Self::build_jwt_handler(config))
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
    ///
    /// TODO: 考虑将 refresh_token 始终使用不透明格式（UUID），即使在 JWT 模式下。
    /// 理由：refresh_token 每次使用都需要服务端验证（黑名单检查 + 会话匹配），
    /// JWT 自包含的优势在 refresh 场景下无意义，反而增加了 token 长度和泄漏风险。
    /// 参考 sa-token-rust 的设计：refresh_token 格式固定为 `refresh_{ts}_{login_id}_{uuid}`。
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

    /// 标准 UUID v4
    pub fn uuid() -> String {
        Uuid::new_v4().to_string()
    }

    /// 无连字符 UUID（用于 JTI 等内部标识）
    pub fn simple_uuid() -> String {
        Uuid::new_v4().simple().to_string()
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
            )
        } else {
            let (encoding_key, decoding_key) = load_asymmetric_keys(config);
            JwtHandler::new(algorithm, encoding_key, decoding_key)
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
            let ek =
                EncodingKey::from_rsa_pem(&private_pem).expect("Invalid RSA private key PEM");
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
            let dk =
                DecodingKey::from_ed_pem(&public_pem).expect("Invalid Ed25519 public key PEM");
            (ek, dk)
        }
        _ => unreachable!("symmetric algorithms handled by caller"),
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
        assert_eq!(pair.access.token.split('.').count(), 3);
        assert_eq!(pair.refresh.token.split('.').count(), 3);
        assert!(pair.access.jti.is_some());
        assert!(pair.refresh.jti.is_some());
        assert_ne!(pair.access.jti, pair.refresh.jti);
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
        // 确保所有 JwtAlgorithm 变体都有映射
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
            let _ = map_algorithm(v); // 不 panic 即通过
        }
    }
}
