use super::MaskingAlgorithm;

#[derive(Debug, Clone, Default)]
pub struct EmailMasking;

impl MaskingAlgorithm for EmailMasking {
    fn mask(&self, input: &str) -> String {
        let Some((user, domain)) = input.split_once('@') else {
            return input.to_string();
        };
        let prefix = user.chars().next().unwrap_or('*');
        format!("{prefix}***@{domain}")
    }

    fn algorithm_type(&self) -> &str {
        "email"
    }
}

#[cfg(test)]
mod tests {
    use crate::masking::{EmailMasking, MaskingAlgorithm};

    #[test]
    fn email_masking_masks_user_part() {
        assert_eq!(EmailMasking.mask("user@example.com"), "u***@example.com");
    }
}
