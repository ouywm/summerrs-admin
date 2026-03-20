use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketConnectAuthDto {
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketSessionState {
    pub socket_id: String,
    pub namespace: String,
    pub login_id: String,
    pub user_id: i64,
    pub user_type: String,
    pub device: String,
    pub user_name: String,
    pub nick_name: String,
    pub connected_at: i64,
}

/// 存入 socketioxide per-socket extensions，断连时不依赖 Redis
#[derive(Debug, Clone)]
pub struct SocketIdentity {
    pub login_id: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KickoutPayload {
    pub reason: String,
    pub message: String,
}
