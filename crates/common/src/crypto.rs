//! 密码哈希与验证工具（基于 Argon2id）

use anyhow::{Context, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

/// 默认密码
pub const DEFAULT_PASSWORD: &str = "123456";

/// 使用 Argon2id 对明文密码进行哈希，返回 PHC 格式字符串
pub fn hash_password(raw: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(raw.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("密码哈希失败")?;
    Ok(hash.to_string())
}

/// 验证明文密码是否与哈希匹配
pub fn verify_password(raw: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("解析哈希字符串失败")?;
    Ok(Argon2::default()
        .verify_password(raw.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_ok() {
        let raw = "P@ssw0rd123";
        let hashed = hash_password(raw).unwrap();
        assert!(verify_password(raw, &hashed).unwrap());
    }

    #[test]
    fn wrong_password_returns_false() {
        let hashed = hash_password("correct").unwrap();
        assert!(!verify_password("wrong", &hashed).unwrap());
    }

    #[test]
    fn empty_password_ok() {
        let hashed = hash_password("").unwrap();
        assert!(verify_password("", &hashed).unwrap());
    }

    #[test]
    fn unicode_password() {
        let raw = "你好世界🌍密码";
        let hashed = hash_password(raw).unwrap();
        assert!(verify_password(raw, &hashed).unwrap());
    }

    #[test]
    fn long_password() {
        let raw = "a".repeat(256);
        let hashed = hash_password(&raw).unwrap();
        assert!(verify_password(&raw, &hashed).unwrap());
    }

    #[test]
    fn invalid_hash_returns_error() {
        let result = verify_password("password", "not-a-valid-hash");
        assert!(result.is_err());
    }

    #[test]
    fn different_passwords_different_hashes() {
        let h1 = hash_password("alpha").unwrap();
        let h2 = hash_password("beta").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn print_hash_123456() {
        let hashed = hash_password("123456").unwrap();
        println!("\n========================================");
        println!("密码 123456 的 Argon2id 哈希值:");
        println!("{hashed}");
        println!("========================================\n");
        assert!(verify_password("123456", &hashed).unwrap());
    }

    #[test]
    fn same_password_different_hashes_due_to_random_salt() {
        let h1 = hash_password("same").unwrap();
        let h2 = hash_password("same").unwrap();
        assert_ne!(h1, h2);
        // 但两者都应该能验证成功
        assert!(verify_password("same", &h1).unwrap());
        assert!(verify_password("same", &h2).unwrap());
    }
}
