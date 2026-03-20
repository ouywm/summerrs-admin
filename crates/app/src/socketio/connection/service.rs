use chrono::Utc;
use common::error::ApiResult;
use std::str::FromStr;
use summer::plugin::Service;
use summer_auth::{LoginId, SessionManager};
use summer_web::socketioxide::socket::Sid;
use summer_web::socketioxide::SocketIo;

use super::super::core::model::SocketSessionState;
use super::session::SocketSessionStore;

// 重导出：保持外部 `use crate::socketio::service::{...}` 可用
pub use super::super::core::model::{KickoutPayload, SocketConnectAuthDto, SocketIdentity};

#[derive(Clone, Service)]
pub struct SocketGatewayService {
    #[inject(component)]
    io: SocketIo,
    #[inject(component)]
    auth: SessionManager,
    #[inject(component)]
    sessions: SocketSessionStore,
}

impl SocketGatewayService {
    pub async fn authenticate_connection(
        &self,
        socket_id: &str,
        namespace: &str,
        access_token: &str,
    ) -> ApiResult<(SocketSessionState, SocketIdentity)> {
        let validated = self.auth.validate_token(access_token).await?;

        let login_id = validated.login_id.encode();
        let identity = SocketIdentity {
            login_id: login_id.clone(),
            roles: validated.roles.clone(),
        };

        let now = Utc::now().timestamp_millis();
        let session = SocketSessionState {
            socket_id: socket_id.to_string(),
            namespace: namespace.to_string(),
            login_id,
            user_id: validated.login_id.user_id,
            user_type: validated.login_id.user_type.to_string(),
            device: validated.device.to_string(),
            user_name: validated.user_name,
            nick_name: validated.nick_name,
            connected_at: now,
        };

        self.sessions.store(&session).await?;
        Ok((session, identity))
    }

    pub async fn unregister_connection(
        &self,
        socket_id: &str,
        namespace: &str,
        login_id: Option<&str>,
    ) -> ApiResult<()> {
        self.sessions.cleanup(socket_id, namespace, login_id).await
    }

    #[allow(dead_code)]
    pub async fn disconnect_by_login_id(&self, login_id: &LoginId) -> ApiResult<usize> {
        self.disconnect_login_sockets(login_id, None, None).await
    }

    pub async fn notify_and_disconnect(
        &self,
        login_id: &LoginId,
        payload: &KickoutPayload,
    ) -> ApiResult<usize> {
        self.disconnect_login_sockets(login_id, None, Some(payload))
            .await
    }

    pub async fn notify_and_disconnect_device(
        &self,
        login_id: &LoginId,
        device: &str,
        payload: &KickoutPayload,
    ) -> ApiResult<usize> {
        self.disconnect_login_sockets(login_id, Some(device), Some(payload))
            .await
    }

    /// GC 委托
    pub async fn gc_stale_index_entries(&self) -> ApiResult<usize> {
        self.sessions.gc_stale_entries().await
    }

    async fn disconnect_login_sockets(
        &self,
        login_id: &LoginId,
        device_filter: Option<&str>,
        notify: Option<&KickoutPayload>,
    ) -> ApiResult<usize> {
        let encoded = login_id.encode();
        let login_index_key = self.sessions.login_index_key(&encoded);
        let socket_ids = self.sessions.socket_ids_by_login(&encoded).await?;

        if socket_ids.is_empty() {
            return Ok(0);
        }

        let sessions = self.sessions.get_batch(&socket_ids).await?;
        let mut disconnected = 0;
        let mut stale_socket_ids = Vec::new();
        let mut cleanup_sessions = Vec::new();

        for (socket_id, session) in socket_ids.into_iter().zip(sessions.into_iter()) {
            let Some(session) = session else {
                stale_socket_ids.push(socket_id);
                continue;
            };

            if let Some(device) = device_filter {
                if session.device != device {
                    continue;
                }
            }

            if let Some(socket) = self.find_socket(&session.namespace, &session.socket_id) {
                if let Some(payload) = &notify {
                    if let Err(err) = socket.emit(
                        super::super::core::event::SESSION_KICKOUT,
                        payload,
                    ) {
                        tracing::warn!(
                            socket_id = %session.socket_id,
                            event = super::super::core::event::SESSION_KICKOUT,
                            error = %err,
                            "Socket emit before disconnect failed"
                        );
                    }
                }

                if let Err(err) = socket.disconnect() {
                    tracing::warn!(
                        socket_id = %session.socket_id,
                        namespace = %session.namespace,
                        error = %err,
                        "Socket disconnect failed, cleaning session eagerly"
                    );
                } else {
                    disconnected += 1;
                }
            }

            cleanup_sessions.push(session);
        }

        self.sessions
            .cleanup_stale_login_index(&login_index_key, &stale_socket_ids)
            .await?;
        self.sessions.cleanup_batch(&cleanup_sessions).await?;

        Ok(disconnected)
    }

    fn find_socket(
        &self,
        namespace: &str,
        socket_id: &str,
    ) -> Option<summer_web::socketioxide::extract::SocketRef> {
        let sid = Sid::from_str(socket_id).ok()?;
        self.io.of(namespace)?.get_socket(sid)
    }
}
