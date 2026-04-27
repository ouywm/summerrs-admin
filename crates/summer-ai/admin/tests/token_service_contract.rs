use summer_ai_admin::service::token_service::{
    new_raw_token, sha256_hex, token_cache_key, token_prefix,
};

#[test]
fn sha256_hex_matches_relay_hash_contract() {
    assert_eq!(
        sha256_hex("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn new_raw_token_has_sk_prefix_and_high_entropy_body() {
    let token = new_raw_token();
    assert!(token.starts_with("sk-"));
    assert_eq!(token.len(), 67);
    assert!(token[3..].chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn token_prefix_is_short_display_prefix() {
    assert_eq!(token_prefix("sk-abcdefghijklmnopqrstuvwxyz"), "sk-abcdefgh");
    assert_eq!(token_prefix("short"), "short");
}

#[test]
fn token_cache_key_matches_relay_cache_key_contract() {
    assert_eq!(token_cache_key("abc123"), "ai:tk:abc123");
}
