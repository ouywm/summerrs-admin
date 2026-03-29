mod aes;
mod digest;

pub use aes::AesGcmEncryptor;
pub use digest::DigestAlgorithm;

use crate::{config::EncryptRuleConfig, error::Result};

pub trait EncryptAlgorithm: Send + Sync + 'static {
    fn encrypt(&self, plaintext: &str) -> Result<String>;
    fn decrypt(&self, ciphertext: &str) -> Result<String>;
}

pub fn lookup_rule<'a>(
    rules: &'a [EncryptRuleConfig],
    table: &str,
    column: &str,
) -> Option<&'a EncryptRuleConfig> {
    rules.iter().find(|rule| {
        rule.table.eq_ignore_ascii_case(table) && rule.column.eq_ignore_ascii_case(column)
    })
}
