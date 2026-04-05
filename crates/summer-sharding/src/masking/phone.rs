use super::MaskingAlgorithm;

#[derive(Debug, Clone, Default)]
pub struct PhoneMasking;

impl MaskingAlgorithm for PhoneMasking {
    fn mask(&self, input: &str) -> String {
        let chars = input.chars().collect::<Vec<_>>();
        if chars.len() < 7 {
            return input.to_string();
        }
        let prefix = chars[..3].iter().collect::<String>();
        let suffix = chars[chars.len() - 4..].iter().collect::<String>();
        format!("{prefix}****{suffix}")
    }

    fn algorithm_type(&self) -> &str {
        "phone"
    }
}

#[cfg(test)]
mod tests {
    use crate::masking::{MaskingAlgorithm, PhoneMasking};

    #[test]
    fn phone_masking_masks_middle_digits() {
        assert_eq!(PhoneMasking.mask("13812341234"), "138****1234");
    }

    #[test]
    fn phone_masking_handles_utf8_without_panicking() {
        let result = std::panic::catch_unwind(|| PhoneMasking.mask("中文中文中文"));

        assert!(result.is_ok(), "utf8 phone masking should not panic");
        assert_eq!(result.unwrap(), "中文中文中文");
    }
}
