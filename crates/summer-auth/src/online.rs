use serde::{Deserialize, Serialize};

use crate::error::AuthResult;
use crate::session::SessionManager;
use crate::session::model::DeviceInfo;
use crate::user_type::{LoginId, UserType};

/// 在线用户查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct OnlineUserQuery {
    pub user_type: Option<UserType>,
    pub page: usize,
    pub page_size: usize,
}

/// 在线用户条目
#[derive(Debug, Clone, Serialize)]
pub struct OnlineUserItem {
    pub login_id: String,
    pub user_type: UserType,
    pub device: String,
    pub login_time: i64,
    pub login_ip: String,
}

/// 在线用户分页结果
#[derive(Debug, Clone, Serialize)]
pub struct OnlineUserPage {
    pub total: usize,
    pub items: Vec<OnlineUserItem>,
}

impl SessionManager {
    /// 查询在线用户（通过扫描 auth:device: 前缀）
    pub async fn online_users(&self, query: OnlineUserQuery) -> AuthResult<OnlineUserPage> {
        // 确定扫描的前缀
        let prefixes: Vec<String> = match query.user_type {
            Some(ut) => {
                // 扫描特定用户类型的所有设备
                vec![format!("auth:device:{}:", ut)]
            }
            None => {
                // 扫描所有用户类型
                UserType::all()
                    .iter()
                    .map(|ut| format!("auth:device:{}:", ut))
                    .collect()
            }
        };

        // 收集所有在线设备
        let mut items = Vec::new();
        for prefix in &prefixes {
            let keys = self.storage.keys_by_prefix(prefix).await?;
            for key in keys {
                // 从 key 解析 login_id 和 device
                if let Some((login_id, device_str)) = parse_device_key(&key) {
                    if let Ok(Some(json)) = self.storage.get_string(&key).await {
                        if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                            items.push(OnlineUserItem {
                                login_id: login_id.encode(),
                                user_type: login_id.user_type,
                                device: device_str,
                                login_time: info.login_time,
                                login_ip: info.login_ip,
                            });
                        }
                    }
                }
            }
        }

        // 按 login_time 降序排序
        items.sort_by(|a, b| b.login_time.cmp(&a.login_time));

        let total = items.len();

        // 分页
        let page = query.page.max(1);
        let start = (page - 1) * query.page_size;
        let paged: Vec<OnlineUserItem> = items
            .into_iter()
            .skip(start)
            .take(query.page_size)
            .collect();

        Ok(OnlineUserPage {
            total,
            items: paged,
        })
    }
}

/// 从 device key 解析 LoginId 和设备名
/// key 格式: auth:device:{user_type}:{user_id}:{device}
fn parse_device_key(key: &str) -> Option<(LoginId, String)> {
    let rest = key.strip_prefix("auth:device:")?;
    // rest = "{user_type}:{user_id}:{device}"
    let (login_str, device_str) = rest.rsplit_once(':')?;
    let login_id = LoginId::decode(login_str)?;
    Some((login_id, device_str.to_string()))
}
