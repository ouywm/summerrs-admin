use std::str::FromStr;

use serde::Serialize;
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_web::config::SocketIOConfig;
use summer_web::socketioxide::SocketIo;
use summer_web::socketioxide::extract::SocketRef;
use summer_web::socketioxide::socket::Sid;

use super::room;

/// 通用 Socket.IO 推送服务
///
/// 业务代码注入此服务即可向 user / role / 全体 / 单 socket 推送任意事件，
/// 无需直接操作底层 `SocketIo` 实例。
#[derive(Clone, Service)]
pub struct SocketEmitter {
    #[inject(component)]
    io: SocketIo,
    #[inject(config)]
    config: SocketIOConfig,
}

impl SocketEmitter {
    /// 向指定用户推送（`user:{user_id}` room）
    pub async fn emit_to_user<T: Serialize>(
        &self,
        user_id: i64,
        event: &str,
        data: &T,
    ) -> ApiResult<()> {
        self.emit_to_room(&room::user_room(user_id), event, data)
            .await
    }

    #[allow(dead_code)]
    /// 向指定角色推送（`role:{role}` room）
    pub async fn emit_to_role<T: Serialize>(
        &self,
        role: &str,
        event: &str,
        data: &T,
    ) -> ApiResult<()> {
        self.emit_to_room(&room::role_room(role), event, data).await
    }

    #[allow(dead_code)]
    /// 向全体推送（`all-{user_type}` room）
    pub async fn emit_broadcast<T: Serialize>(
        &self,
        user_type: &str,
        event: &str,
        data: &T,
    ) -> ApiResult<()> {
        self.emit_to_room(&room::broadcast_room(user_type), event, data)
            .await
    }

    #[allow(dead_code)]
    /// 向指定 socket 推送（按 socket_id 精确投递，使用默认 namespace）
    pub fn emit_to_socket<T: Serialize>(
        &self,
        socket_id: &str,
        event: &str,
        data: &T,
    ) -> ApiResult<()> {
        if let Some(socket) = self.get_socket(self.ns(), socket_id) {
            socket
                .emit(event, data)
                .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        }
        Ok(())
    }

    /// 查找内存中的 socket 实例（按 namespace + socket_id）
    pub(crate) fn get_socket(&self, namespace: &str, socket_id: &str) -> Option<SocketRef> {
        let sid = Sid::from_str(socket_id).ok()?;
        self.io.of(namespace)?.get_socket(sid)
    }

    /// 默认 namespace
    fn ns(&self) -> &str {
        self.config.default_namespace.as_str()
    }

    /// 底层方法：向指定 room 推送
    async fn emit_to_room<T: Serialize>(&self, room: &str, event: &str, data: &T) -> ApiResult<()> {
        if let Some(ns_ref) = self.io.of(self.ns()) {
            ns_ref
                .within(room.to_string())
                .emit(event, data)
                .await
                .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        }
        Ok(())
    }
}
