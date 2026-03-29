use super::MaskingAlgorithm;

#[derive(Debug, Clone, Default)]
pub struct PhoneMasking;

impl MaskingAlgorithm for PhoneMasking {
    fn mask(&self, input: &str) -> String {
        if input.len() < 7 {
            return input.to_string();
        }
        format!("{}****{}", &input[..3], &input[input.len() - 4..])
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
}
