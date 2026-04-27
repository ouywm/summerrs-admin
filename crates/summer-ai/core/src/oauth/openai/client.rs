use super::types::{OpenAiForm, OpenAiOAuthError, OpenAiTokenResponse};

const OPENAI_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_OAUTH_SCOPE: &str = "openid profile email";

pub fn build_exchange_form(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_id: &str,
) -> OpenAiForm {
    let mut form = OpenAiForm::new();
    form.insert("grant_type".into(), "authorization_code".into());
    form.insert("code".into(), code.into());
    form.insert("code_verifier".into(), code_verifier.into());
    form.insert("redirect_uri".into(), redirect_uri.into());
    form.insert("client_id".into(), client_id.into());
    form
}

pub fn build_refresh_form(client_id: &str, refresh_token: &str) -> OpenAiForm {
    let mut form = OpenAiForm::new();
    form.insert("grant_type".into(), "refresh_token".into());
    form.insert("client_id".into(), client_id.into());
    form.insert("refresh_token".into(), refresh_token.into());
    form.insert("scope".into(), OPENAI_OAUTH_SCOPE.into());
    form
}

pub async fn exchange_code(
    http: &reqwest::Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_id: &str,
) -> Result<OpenAiTokenResponse, OpenAiOAuthError> {
    let form = build_exchange_form(code, code_verifier, redirect_uri, client_id);
    send_token_request(http, form).await
}

pub async fn refresh_token(
    http: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<OpenAiTokenResponse, OpenAiOAuthError> {
    let form = build_refresh_form(client_id, refresh_token);
    send_token_request(http, form).await
}

async fn send_token_request(
    http: &reqwest::Client,
    form: OpenAiForm,
) -> Result<OpenAiTokenResponse, OpenAiOAuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(
            form.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .finish();
    let response = http
        .post(OPENAI_TOKEN_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(OpenAiOAuthError::Transport)?;
    let status = response.status();
    let body = response.text().await.map_err(OpenAiOAuthError::Transport)?;
    if status.is_success() {
        return serde_json::from_str::<OpenAiTokenResponse>(&body).map_err(|source| {
            OpenAiOAuthError::Decode {
                source,
                context: decode_context(&body),
            }
        });
    }

    let provider_error = serde_json::from_str(&body).ok();
    Err(OpenAiOAuthError::HttpStatus {
        status,
        body,
        provider_error,
    })
}

fn decode_context(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(object) = value.as_object() {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            return format!("json object keys: {}", keys.join(","));
        }
        return format!("json {} bytes", body.len());
    }

    format!("non-json {} bytes", body.len())
}
