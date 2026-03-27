use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::digest::{SHA256, digest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    Sha256,
}

impl DigestAlgorithm {
    pub fn digest(&self, input: &str) -> String {
        match self {
            Self::Sha256 => BASE64.encode(digest(&SHA256, input.as_bytes()).as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::encrypt::DigestAlgorithm;

    #[test]
    fn digest_is_stable() {
        let left = DigestAlgorithm::Sha256.digest("secret");
        let right = DigestAlgorithm::Sha256.digest("secret");
        assert_eq!(left, right);
        assert_ne!(left, DigestAlgorithm::Sha256.digest("other"));
    }
}
