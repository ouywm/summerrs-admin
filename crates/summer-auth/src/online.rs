use serde::{Deserialize, Serialize};

use crate::error::AuthResult;
use crate::session::SessionManager;
use crate::user_type::{DeviceType, LoginId, UserType};

/// 在线用户查询参数
#[derive(Debug, Default)]
pub struct OnlineUserQuery {
    pub user_type: Option<UserType>,
    pub page: usize,
    pub page_size: usize,
}

/// 在线用户分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineUserPage {
    pub total: usize,
    pub items: Vec<OnlineUser>,
}

/// 在线用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineUser {
    pub login_id: LoginId,
    pub device: DeviceType,
    pub login_time: i64,
    pub last_active_time: i64,
    pub login_ip: String,
    pub user_agent: String,
    pub user_name: String,
    pub nick_name: String,
    pub avatar: String,
}

impl SessionManager {
    /// 查询在线用户
    pub async fn online_users(&self, query: OnlineUserQuery) -> AuthResult<OnlineUserPage> {
        // 根据 user_type 确定要扫描的前缀
        let prefixes: Vec<&str> = match query.user_type {
            Some(ut) => vec![ut.prefix()],
            None => UserType::all().iter().map(|t| t.prefix()).collect(),
        };

        let mut all_users = Vec::new();

        for prefix in prefixes {
            let session_prefix = format!("{prefix}:session:");
            let keys = self
                .storage
                .keys_by_prefix(&session_prefix)
                .await?;

            for key in keys {
                if let Ok(Some(session)) = self.storage.get_session(&key).await {
                    for ds in &session.devices {
                        all_users.push(OnlineUser {
                            login_id: session.login_id.clone(),
                            device: ds.device.clone(),
                            login_time: ds.login_time,
                            last_active_time: ds.last_active_time,
                            login_ip: ds.login_ip.clone(),
                            user_agent: ds.user_agent.clone(),
                            user_name: session.profile.user_name().to_string(),
                            nick_name: session.profile.nick_name().to_string(),
                            avatar: session.profile.avatar().to_string(),
                        });
                    }
                }
            }
        }

        // 按最后活跃时间倒序
        all_users.sort_by(|a, b| b.last_active_time.cmp(&a.last_active_time));

        let total = all_users.len();

        // 分页
        let page_size = if query.page_size == 0 {
            20
        } else {
            query.page_size
        };
        let page = if query.page == 0 { 1 } else { query.page };
        let start = (page - 1) * page_size;

        let items = all_users.into_iter().skip(start).take(page_size).collect();

        Ok(OnlineUserPage { total, items })
    }
}
