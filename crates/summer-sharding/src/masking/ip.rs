use super::MaskingAlgorithm;

#[derive(Debug, Clone, Default)]
pub struct IpMasking;

impl MaskingAlgorithm for IpMasking {
    fn mask(&self, input: &str) -> String {
        let parts = input.split('.').collect::<Vec<_>>();
        if parts.len() == 4 {
            format!("{}.{}.*.*", parts[0], parts[1])
        } else {
            input.to_string()
        }
    }

    fn algorithm_type(&self) -> &str {
        "ip"
    }
}

#[cfg(test)]
mod tests {
    use crate::masking::{IpMasking, MaskingAlgorithm};

    #[test]
    fn ip_masking_masks_tail_octets() {
        assert_eq!(IpMasking.mask("192.168.1.20"), "192.168.*.*");
    }
}
