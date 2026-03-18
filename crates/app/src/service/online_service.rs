use std::collections::HashMap;
use std::net::IpAddr;

use common::error::{ApiErrors, ApiResult};
use model::entity::sys_user;
use model::vo::online::OnlineUserVo;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_auth::{LoginId, OnlineUserQuery, SessionManager, UserType};

use crate::plugin::ip2region::Ip2RegionSearcher;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination};

#[derive(Clone, Service)]
pub struct OnlineUserService {
    #[inject(component)]
    auth: SessionManager,
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
}

impl OnlineUserService {
    /// 查询在线用户列表（仅 Admin）
    pub async fn list_online_users(&self, pagination: Pagination) -> ApiResult<Page<OnlineUserVo>> {
        // 分页下沉到 summer-auth 层，只取当前页数据
        let result = self
            .auth
            .online_users(OnlineUserQuery {
                user_type: Some(UserType::Admin),
                page: (pagination.page + 1) as usize, // Pagination 是 0-based，OnlineUserQuery 是 1-based
                page_size: pagination.size as usize,
            })
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;

        // 批量查 DB 补充用户信息（只查当前页涉及的用户）
        let user_ids: Vec<i64> = result
            .items
            .iter()
            .filter_map(|item| LoginId::decode(&item.login_id).map(|lid| lid.user_id))
            .collect();

        let users: HashMap<i64, sys_user::Model> = if !user_ids.is_empty() {
            sys_user::Entity::find()
                .filter(sys_user::Column::Id.is_in(user_ids))
                .all(&self.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|u| (u.id, u))
                .collect()
        } else {
            HashMap::new()
        };

        // 组装 VO
        let items: Vec<OnlineUserVo> = result
            .items
            .iter()
            .filter_map(|item| {
                let login_id = LoginId::decode(&item.login_id)?;
                let user = users.get(&login_id.user_id);
                let location = item
                    .login_ip
                    .parse::<IpAddr>()
                    .map(|ip| self.ip_searcher.search_location(&ip))
                    .unwrap_or_default();
                Some(OnlineUserVo {
                    login_id: item.login_id.clone(),
                    user_id: login_id.user_id,
                    user_name: user.map(|u| u.user_name.clone()).unwrap_or_default(),
                    nick_name: user.map(|u| u.nick_name.clone()).unwrap_or_default(),
                    avatar: user.map(|u| u.avatar.clone()).unwrap_or_default(),
                    device: item.device.clone(),
                    login_time: item.login_time,
                    login_ip: item.login_ip.clone(),
                    login_location: location,
                })
            })
            .collect();

        Ok(Page::new(items, &pagination, result.total as u64))
    }

    /// 强制踢下线（所有设备）
    pub async fn kick_out(&self, login_id_str: &str) -> ApiResult<()> {
        let login_id = LoginId::decode(login_id_str)
            .ok_or_else(|| ApiErrors::BadRequest("无效的 login_id".to_string()))?;
        self.auth
            .kick_out(&login_id, None)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }

    /// 踢指定设备
    pub async fn kick_out_device(&self, login_id_str: &str, device_str: &str) -> ApiResult<()> {
        let login_id = LoginId::decode(login_id_str)
            .ok_or_else(|| ApiErrors::BadRequest("无效的 login_id".to_string()))?;
        let device = summer_auth::DeviceType::from(device_str);
        self.auth
            .kick_out(&login_id, Some(&device))
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }
}
