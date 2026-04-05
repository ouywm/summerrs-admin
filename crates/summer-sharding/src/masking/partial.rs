use super::MaskingAlgorithm;

#[derive(Debug, Clone, Copy)]
pub struct PartialMasking {
    pub keep_start: usize,
    pub keep_end: usize,
    pub mask_char: char,
}

impl MaskingAlgorithm for PartialMasking {
    fn mask(&self, input: &str) -> String {
        let chars = input.chars().collect::<Vec<_>>();
        if chars.len() <= self.keep_start + self.keep_end {
            return input.to_string();
        }

        let start = chars[..self.keep_start].iter().collect::<String>();
        let end = chars[chars.len() - self.keep_end..]
            .iter()
            .collect::<String>();
        let mask = self
            .mask_char
            .to_string()
            .repeat(chars.len().saturating_sub(self.keep_start + self.keep_end));
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

    #[test]
    fn partial_masking_handles_utf8_without_panicking() {
        let masking = PartialMasking {
            keep_start: 1,
            keep_end: 1,
            mask_char: '*',
        };
        let result = std::panic::catch_unwind(|| masking.mask("中间"));

        assert!(result.is_ok(), "utf8 masking should not panic");
        assert_eq!(result.unwrap(), "中间");
    }
}
