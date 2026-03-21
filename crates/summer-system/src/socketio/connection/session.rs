use anyhow::Context;
use summer_common::error::{ApiErrors, ApiResult};
use summer::plugin::Service;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;
use summer_redis::Redis;

use super::super::core::model::SocketSessionState;

#[derive(Clone, Service)]
pub struct SocketSessionStore {
    #[inject(component)]
    redis: Redis,
    #[inject(config)]
    config: super::config::SocketGatewayConfig,
}

impl SocketSessionStore {
    pub async fn store(&self, session: &SocketSessionState) -> ApiResult<()> {
        let raw = serde_json::to_string(session).context("序列化 Socket 会话失败")?;

        let mut redis = self.redis.clone();
        let mut pipeline = redis::pipe();
        pipeline
            .atomic()
            .cmd("SETEX")
            .arg(self.session_key(&session.socket_id))
            .arg(self.config.session_ttl_seconds)
            .arg(raw)
            .ignore()
            .cmd("SADD")
            .arg(self.socket_index_key())
            .arg(&session.socket_id)
            .ignore()
            .cmd("SADD")
            .arg(self.namespace_index_key(&session.namespace))
            .arg(&session.socket_id)
            .ignore()
            .cmd("SADD")
            .arg(self.login_index_key(&session.login_id))
            .arg(&session.socket_id)
            .ignore()
            .query_async::<()>(&mut redis)
            .await
            .context("保存 Socket 连接状态失败")?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get(&self, socket_id: &str) -> ApiResult<Option<SocketSessionState>> {
        let mut redis = self.redis.clone();
        let raw: Option<String> = redis
            .get(self.session_key(socket_id))
            .await
            .context("读取 Socket 会话失败")?;

        raw.map(|value| parse_session_state(&value, socket_id))
            .transpose()
            .map_err(ApiErrors::Internal)
    }

    pub async fn get_batch(
        &self,
        socket_ids: &[String],
    ) -> ApiResult<Vec<Option<SocketSessionState>>> {
        if socket_ids.is_empty() {
            return Ok(Vec::new());
        }

        let keys: Vec<String> = socket_ids
            .iter()
            .map(|socket_id| self.session_key(socket_id))
            .collect();

        let mut redis = self.redis.clone();
        let raws: Vec<Option<String>> = redis::cmd("MGET")
            .arg(&keys)
            .query_async(&mut redis)
            .await
            .context("批量读取 Socket 会话失败")?;

        raws.into_iter()
            .enumerate()
            .map(|(idx, raw)| {
                raw.map(|value| parse_session_state(&value, &socket_ids[idx]))
                    .transpose()
                    .map_err(ApiErrors::Internal)
            })
            .collect()
    }

    pub async fn socket_ids_by_login(&self, login_id: &str) -> ApiResult<Vec<String>> {
        let mut redis = self.redis.clone();
        redis
            .smembers(self.login_index_key(login_id))
            .await
            .context("读取 Socket 登录身份索引失败")
            .map_err(ApiErrors::Internal)
    }

    pub async fn cleanup(&self, socket_id: &str, namespace: &str, login_id: Option<&str>) -> ApiResult<()> {
        let mut redis = self.redis.clone();
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        self.append_cleanup_commands(&mut pipeline, socket_id, namespace, login_id);
        pipeline
            .query_async::<()>(&mut redis)
            .await
            .context("清理 Socket 会话失败")?;

        Ok(())
    }

    pub async fn cleanup_batch(&self, sessions: &[SocketSessionState]) -> ApiResult<()> {
        if sessions.is_empty() {
            return Ok(());
        }

        let mut redis = self.redis.clone();
        let mut pipeline = redis::pipe();
        pipeline.atomic();

        for session in sessions {
            self.append_cleanup_commands(
                &mut pipeline,
                &session.socket_id,
                &session.namespace,
                Some(&session.login_id),
            );
        }

        pipeline
            .query_async::<()>(&mut redis)
            .await
            .context("批量清理 Socket 会话失败")?;

        Ok(())
    }

    pub async fn cleanup_stale_login_index(
        &self,
        login_index_key: &str,
        stale_socket_ids: &[String],
    ) -> ApiResult<()> {
        if stale_socket_ids.is_empty() {
            return Ok(());
        }

        let mut redis = self.redis.clone();
        let _: usize = redis::cmd("SREM")
            .arg(login_index_key)
            .arg(stale_socket_ids)
            .query_async(&mut redis)
            .await
            .context("清理失效 Socket 登录身份索引失败")?;

        Ok(())
    }

    pub fn login_index_key(&self, login_id: &str) -> String {
        format!("{}:login:{login_id}:sockets", self.config.redis_prefix)
    }

    /// GC：扫描全部索引（global/namespace/login），清理 session 已过期的幽灵条目
    pub async fn gc_stale_entries(&self) -> ApiResult<usize> {
        let mut redis = self.redis.clone();

        let ns_pattern = format!("{}:namespace:*:sockets", self.config.redis_prefix);
        let login_pattern = format!("{}:login:*:sockets", self.config.redis_prefix);

        let mut scan_pipeline = redis::pipe();
        scan_pipeline
            .cmd("KEYS")
            .arg(&ns_pattern)
            .cmd("KEYS")
            .arg(&login_pattern);
        let (ns_keys, login_keys): (Vec<String>, Vec<String>) = scan_pipeline
            .query_async(&mut redis)
            .await
            .context("扫描 Socket 索引 key 失败")?;

        let mut all_index_keys = vec![self.socket_index_key()];
        all_index_keys.extend(ns_keys);
        all_index_keys.extend(login_keys);

        let mut cleaned = 0usize;

        for index_key in &all_index_keys {
            let members: Vec<String> = redis
                .smembers(index_key)
                .await
                .context("读取 Socket 索引成员失败")?;

            if members.is_empty() {
                continue;
            }

            let mut check_pipeline = redis::pipe();
            for sid in &members {
                check_pipeline.cmd("EXISTS").arg(self.session_key(sid));
            }
            let exists: Vec<bool> = check_pipeline
                .query_async(&mut redis)
                .await
                .context("批量检查 Socket 会话存在性失败")?;

            let stale: Vec<&str> = members
                .iter()
                .zip(exists.iter())
                .filter(|(_, exists)| !**exists)
                .map(|(sid, _)| sid.as_str())
                .collect();

            if stale.is_empty() {
                continue;
            }

            cleaned += stale.len();
            let _: usize = redis::cmd("SREM")
                .arg(index_key)
                .arg(&stale)
                .query_async(&mut redis)
                .await
                .context("清理过期 Socket 索引条目失败")?;
        }

        if cleaned > 0 {
            tracing::info!("GC 清理了 {} 个过期 Socket 索引条目", cleaned);
        }
        Ok(cleaned)
    }

    fn session_key(&self, socket_id: &str) -> String {
        format!("{}:session:{socket_id}", self.config.redis_prefix)
    }

    fn socket_index_key(&self) -> String {
        format!("{}:sockets", self.config.redis_prefix)
    }

    fn namespace_index_key(&self, namespace: &str) -> String {
        let namespace = normalize_namespace(namespace);
        format!("{}:namespace:{namespace}:sockets", self.config.redis_prefix)
    }

    fn append_cleanup_commands(
        &self,
        pipeline: &mut redis::Pipeline,
        socket_id: &str,
        namespace: &str,
        login_id: Option<&str>,
    ) {
        pipeline
            .cmd("SREM")
            .arg(self.socket_index_key())
            .arg(socket_id)
            .ignore()
            .cmd("SREM")
            .arg(self.namespace_index_key(namespace))
            .arg(socket_id)
            .ignore();

        if let Some(login_id) = login_id {
            pipeline
                .cmd("SREM")
                .arg(self.login_index_key(login_id))
                .arg(socket_id)
                .ignore();
        }

        pipeline
            .cmd("DEL")
            .arg(self.session_key(socket_id))
            .ignore();
    }
}

fn normalize_namespace(namespace: &str) -> String {
    let namespace = namespace.trim_matches('/');
    if namespace.is_empty() {
        "root".to_string()
    } else {
        namespace.replace('/', ":")
    }
}

fn parse_session_state(raw: &str, socket_id: &str) -> anyhow::Result<SocketSessionState> {
    serde_json::from_str(raw).with_context(|| format!("解析 Socket 会话失败: {socket_id}"))
}


#[cfg(test)]
mod tests {
    use super::normalize_namespace;

    #[test]
    fn normalize_root_namespace() {
        assert_eq!(normalize_namespace("/"), "root");
        assert_eq!(normalize_namespace(""), "root");
    }

    #[test]
    fn normalize_nested_namespace() {
        assert_eq!(normalize_namespace("/admin"), "admin");
        assert_eq!(normalize_namespace("/notice/center"), "notice:center");
    }
}
