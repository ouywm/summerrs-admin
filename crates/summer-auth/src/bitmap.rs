use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::HashMap;

/// 权限映射表：auth_mark <-> bit_position
#[derive(Debug, Clone)]
pub struct PermissionMap {
    /// auth_mark -> bit_position
    forward: HashMap<String, u32>,
    /// bit_position -> auth_mark
    reverse: HashMap<u32, String>,
}

impl PermissionMap {
    pub fn new(mapping: Vec<(String, u32)>) -> Self {
        let forward: HashMap<String, u32> = mapping.iter().cloned().collect();
        let reverse: HashMap<u32, String> =
            mapping.into_iter().map(|(perm, pos)| (pos, perm)).collect();
        Self { forward, reverse }
    }

    pub fn get_position(&self, perm: &str) -> Option<u32> {
        self.forward.get(perm).copied()
    }

    pub fn get_perm(&self, pos: u32) -> Option<&str> {
        self.reverse.get(&pos).map(|s| s.as_str())
    }
}

/// 编码: Vec<String> -> Base64 bitmap, 无有效权限时返回 None
pub fn encode(permissions: &[String], map: &PermissionMap) -> Option<String> {
    let positions: Vec<u32> = permissions
        .iter()
        .filter_map(|p| map.get_position(p))
        .collect();
    if positions.is_empty() {
        return None;
    }
    let max_bit = *positions.iter().max().unwrap();
    let byte_len = (max_bit as usize / 8) + 1;
    let mut bytes = vec![0u8; byte_len];
    for pos in positions {
        bytes[pos as usize / 8] |= 1 << (pos % 8);
    }
    Some(STANDARD.encode(&bytes))
}

/// 解码: Base64 bitmap -> Vec<String>
pub fn decode(pb: &str, map: &PermissionMap) -> Vec<String> {
    let bytes = STANDARD.decode(pb).unwrap_or_default();
    let mut perms = Vec::new();
    for (byte_idx, &byte) in bytes.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                let pos = (byte_idx * 8 + bit) as u32;
                if let Some(perm) = map.get_perm(pos) {
                    perms.push(perm.to_string());
                }
            }
        }
    }
    perms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> PermissionMap {
        PermissionMap::new(vec![
            ("system:user:list".to_string(), 0),
            ("system:user:add".to_string(), 1),
            ("system:user:edit".to_string(), 2),
            ("system:user:delete".to_string(), 3),
            ("system:role:list".to_string(), 4),
            ("system:role:add".to_string(), 5),
        ])
    }

    #[test]
    fn encode_decode_roundtrip() {
        let map = test_map();
        let perms = vec![
            "system:user:list".to_string(),
            "system:user:add".to_string(),
            "system:role:list".to_string(),
        ];
        let encoded = encode(&perms, &map).unwrap();
        let decoded = decode(&encoded, &map);

        // 排序后比较（bitmap 解码顺序按 bit 位置）
        let mut expected = perms.clone();
        expected.sort();
        let mut actual = decoded;
        actual.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_permissions() {
        let map = test_map();
        let perms: Vec<String> = vec![];
        let encoded = encode(&perms, &map);
        assert!(encoded.is_none());
    }

    #[test]
    fn unknown_perm_skipped() {
        let map = test_map();
        // 全部不在映射表中 → None
        let perms = vec!["unknown:perm".to_string()];
        assert!(encode(&perms, &map).is_none());

        // 部分在映射表中
        let perms = vec![
            "system:user:list".to_string(),
            "unknown:perm".to_string(),
        ];
        let encoded = encode(&perms, &map).unwrap();
        let decoded = decode(&encoded, &map);
        assert_eq!(decoded, vec!["system:user:list"]);
    }

    #[test]
    fn bitmap_size_is_compact() {
        // 100 个权限的映射表
        let mapping: Vec<(String, u32)> = (0..100)
            .map(|i| (format!("perm:{}", i), i))
            .collect();
        let map = PermissionMap::new(mapping);

        // 全部 100 个权限
        let perms: Vec<String> = (0..100).map(|i| format!("perm:{}", i)).collect();
        let encoded = encode(&perms, &map).unwrap();

        // Base64 解码后的原始字节数应 <= 20 字节（100 bits = 13 bytes）
        let raw_bytes = STANDARD.decode(&encoded).unwrap();
        assert!(raw_bytes.len() <= 20, "bitmap size: {} bytes", raw_bytes.len());

        // roundtrip 验证
        let decoded = decode(&encoded, &map);
        let mut expected = perms;
        expected.sort();
        let mut actual = decoded;
        actual.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn single_permission_at_high_bit() {
        let map = PermissionMap::new(vec![("high:perm".to_string(), 63)]);
        let perms = vec!["high:perm".to_string()];
        let encoded = encode(&perms, &map).unwrap();
        let decoded = decode(&encoded, &map);
        assert_eq!(decoded, vec!["high:perm"]);
    }
}
