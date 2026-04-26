use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

const OPENAI_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";

pub fn generate_session_id() -> Result<String, uuid::Error> {
    Ok(Uuid::new_v4().as_simple().to_string())
}

pub fn generate_state() -> Result<String, uuid::Error> {
    Ok(Uuid::new_v4().as_simple().to_string())
}

pub fn generate_code_verifier() -> Result<String, uuid::Error> {
    Ok(format!(
        "{}{}",
        Uuid::new_v4().as_simple(),
        Uuid::new_v4().as_simple()
    ))
}

pub fn generate_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn build_authorization_url(
    state: &str,
    code_challenge: &str,
    redirect_uri: &str,
    client_id: &str,
) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(OPENAI_AUTHORIZE_URL)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true");

    Ok(url)
}
