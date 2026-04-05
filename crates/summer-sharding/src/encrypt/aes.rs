use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hkdf::{HKDF_SHA256, KeyType, Salt};
use ring::rand::{SecureRandom, SystemRandom};

use crate::{
    encrypt::EncryptAlgorithm,
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub struct AesGcmEncryptor {
    key: [u8; 32],
}

const HKDF_SALT: &[u8] = b"summer-sharding:aes-gcm";
const HKDF_INFO: &[u8] = b"summer-sharding:aes-gcm:key:v1";

#[derive(Debug, Clone, Copy)]
struct Aes256KeyLen;

impl KeyType for Aes256KeyLen {
    fn len(&self) -> usize {
        32
    }
}

impl AesGcmEncryptor {
    pub fn from_env(key_env: &str) -> Result<Self> {
        let raw = std::env::var(key_env)
            .map_err(|_| ShardingError::Config(format!("missing encryption env `{key_env}`")))?;
        Self::from_material(raw.as_bytes())
    }

    pub fn from_material(material: &[u8]) -> Result<Self> {
        if material.is_empty() {
            return Err(ShardingError::Config(
                "encryption key material cannot be empty".to_string(),
            ));
        }
        let salt = Salt::new(HKDF_SHA256, HKDF_SALT);
        let prk = salt.extract(material);
        let okm = prk
            .expand(&[HKDF_INFO], Aes256KeyLen)
            .map_err(|_| ShardingError::Config("invalid encryption key material".to_string()))?;
        let mut key = [0_u8; 32];
        okm.fill(&mut key)
            .map_err(|_| ShardingError::Config("invalid encryption key material".to_string()))?;
        Ok(Self { key })
    }
}

impl EncryptAlgorithm for AesGcmEncryptor {
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.key)
            .map_err(|_| ShardingError::Rewrite("invalid AES-256-GCM key".to_string()))?;
        let key = LessSafeKey::new(unbound);
        let mut nonce_bytes = [0_u8; 12];
        SystemRandom::new().fill(&mut nonce_bytes).map_err(|_| {
            ShardingError::Rewrite("AES-256-GCM nonce generation failed".to_string())
        })?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut buffer = plaintext.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut buffer)
            .map_err(|_| ShardingError::Rewrite("AES-256-GCM encryption failed".to_string()))?;
        let mut payload = nonce_bytes.to_vec();
        payload.extend(buffer);
        Ok(BASE64.encode(payload))
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.key)
            .map_err(|_| ShardingError::Rewrite("invalid AES-256-GCM key".to_string()))?;
        let key = LessSafeKey::new(unbound);
        let mut payload = BASE64
            .decode(ciphertext.as_bytes())
            .map_err(|err| ShardingError::Rewrite(err.to_string()))?;
        if payload.len() < 12 {
            return Err(ShardingError::Rewrite(
                "ciphertext payload is shorter than AES-GCM nonce".to_string(),
            ));
        }
        let nonce = Nonce::assume_unique_for_key(
            payload[..12]
                .try_into()
                .map_err(|_| ShardingError::Rewrite("invalid AES-GCM nonce".to_string()))?,
        );
        let mut buffer = payload.split_off(12);
        let plain = key
            .open_in_place(nonce, Aad::empty(), &mut buffer)
            .map_err(|_| ShardingError::Rewrite("AES-256-GCM decrypt failed".to_string()))?;
        String::from_utf8(plain.to_vec()).map_err(|err| ShardingError::Rewrite(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use crate::encrypt::{AesGcmEncryptor, EncryptAlgorithm};

    #[test]
    fn aes_gcm_round_trip() {
        let encryptor =
            AesGcmEncryptor::from_material(b"12345678901234567890123456789012").expect("encryptor");
        let cipher = encryptor.encrypt("secret").expect("cipher");
        let plain = encryptor.decrypt(cipher.as_str()).expect("plain");
        assert_eq!(plain, "secret");
    }

    #[test]
    fn short_material_is_not_zero_padded_directly() {
        let encryptor = AesGcmEncryptor::from_material(b"short").expect("encryptor");
        let mut zero_padded = [0_u8; 32];
        zero_padded[..5].copy_from_slice(b"short");

        assert_ne!(
            encryptor.key, zero_padded,
            "short key material should be derived, not copied and zero-padded"
        );
        assert_eq!(
            encryptor.key,
            AesGcmEncryptor::from_material(b"short")
                .expect("encryptor")
                .key
        );
    }
}
