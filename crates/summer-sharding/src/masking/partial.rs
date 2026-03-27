use super::MaskingAlgorithm;

#[derive(Debug, Clone, Copy)]
pub struct PartialMasking {
    pub keep_start: usize,
    pub keep_end: usize,
    pub mask_char: char,
}

impl MaskingAlgorithm for PartialMasking {
    fn mask(&self, input: &str) -> String {
        if input.len() <= self.keep_start + self.keep_end {
            return input.to_string();
        }
        let start = &input[..self.keep_start];
        let end = &input[input.len() - self.keep_end..];
        let mask = self
            .mask_char
            .to_string()
            .repeat(input.len().saturating_sub(self.keep_start + self.keep_end));
        format!("{start}{mask}{end}")
    }

    fn algorithm_type(&self) -> &str {
        "partial"
    }
}

#[cfg(test)]
mod tests {
    use crate::masking::{MaskingAlgorithm, PartialMasking};

    #[test]
    fn partial_masking_keeps_ends() {
        let masking = PartialMasking {
            keep_start: 2,
            keep_end: 2,
            mask_char: '*',
        };
        assert_eq!(masking.mask("abcdef"), "ab**ef");
    }
}
