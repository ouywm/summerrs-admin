#[derive(Clone, Debug)]
pub struct OpenAiOAuthSession {
    pub state: String,
    pub code_verifier: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
