use chrono::Utc;
use summer::plugin::Service;
use summer_auth::{LoginId, SessionManager};
use summer_common::error::ApiResult;

use super::super::core::{emitter::SocketEmitter, event, model::SocketSessionState};
use super::session::SocketSessionStore;

// 重导出：保持外部 `use crate::socketio::service::{...}` 可用
pub use super::super::core::model::{KickoutPayload, SocketConnectAuthDto, SocketIdentity};

#[derive(Clone, Service)]
pub struct SocketGatewayService {
    #[inject(component)]
    auth: SessionManager,
    #[inject(component)]
    sessions: SocketSessionStore,
    #[inject(component)]
    emitter: SocketEmitter,
}

impl SocketGatewayService {
    /// 认证新连接：校验 token，创建会话并存入 Redis
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

    /// 注销连接：清理 Redis 中的会话和索引
    pub async fn unregister_connection(
        &self,
        socket_id: &str,
        namespace: &str,
        login_id: Option<&str>,
    ) -> ApiResult<()> {
        self.sessions.cleanup(socket_id, namespace, login_id).await
    }

    /// 静默断开该用户的所有 socket（不推送事件）
    #[allow(dead_code)]
    pub async fn disconnect_by_login_id(&self, login_id: &LoginId) -> ApiResult<usize> {
        self.disconnect_login_sockets(login_id, None, None).await
    }

    /// 推送踢出事件后断开该用户的所有 socket
    ///
    /// 通过 room 广播推送事件（一次 emit 覆盖所有设备），
    /// 然后遍历断开各 socket 并清理 Redis。
    pub async fn notify_and_disconnect(
        &self,
        login_id: &LoginId,
        payload: &KickoutPayload,
    ) -> ApiResult<usize> {
        self.emitter
            .emit_to_user(login_id.user_id, event::SESSION_KICKOUT, payload)
            .await?;
        self.disconnect_login_sockets(login_id, None, None).await
    }

    /// 推送踢出事件后断开该用户指定设备的 socket
    ///
    /// 单设备踢出无法使用 room 广播（会误通知其他设备），
    /// 由 `disconnect_login_sockets` 在循环内按 socket 精确推送。
    pub async fn notify_and_disconnect_device(
        &self,
        login_id: &LoginId,
        device: &str,
        payload: &KickoutPayload,
    ) -> ApiResult<usize> {
        self.disconnect_login_sockets(login_id, Some(device), Some(payload))
            .await
    }

    /// GC 委托：清理过期的 socket 索引条目
    pub async fn gc_stale_index_entries(&self) -> ApiResult<usize> {
        self.sessions.gc_stale_entries().await
    }

    /// 内部统一方法：按 login_id 查找 socket，可选设备过滤，可选 per-socket 推送后断开
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

            if let Some(device) = device_filter
                && session.device != device
            {
                continue;
            }

            if let Some(socket) = self
                .emitter
                .get_socket(&session.namespace, &session.socket_id)
            {
                if let Some(payload) = notify
                    && let Err(err) = socket.emit(event::SESSION_KICKOUT, payload)
                {
                    tracing::warn!(
                        socket_id = %session.socket_id,
                        event = event::SESSION_KICKOUT,
                        error = %err,
                        "Socket emit before disconnect failed"
                    );
                }

                match socket.disconnect() {
                    Ok(()) => disconnected += 1,
                    Err(err) => tracing::warn!(
                        socket_id = %session.socket_id,
                        namespace = %session.namespace,
                        error = %err,
                        "Socket disconnect failed, cleaning session eagerly"
                    ),
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
}
