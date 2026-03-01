use crate::plugin::ip2region_plugin::Ip2RegionSearcher;
use crate::plugin::pagination::{Page, Pagination, PaginationExt};
use crate::plugin::sea_orm_plugin::DbConn;
use anyhow::Context;
use common::error::ApiResult;
use common::user_agent::UserAgentInfo;
use model::dto::login_log::{CreateLoginLogDto, LoginLogQueryDto};
use model::entity::sys_login_log;
use model::vo::login_log::LoginLogVo;
use sea_orm::sea_query::{Alias, Expr};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ExprTrait, QueryFilter, QueryOrder};
use spring::plugin::Service;
use std::net::IpAddr;

#[derive(Clone, Service)]
pub struct LoginLogService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
}

impl LoginLogService {
    /// 记录登录日志（异步，不阻塞主流程）
    pub fn record_login_async(
        &self,
        user_id: i64,
        user_name: String,
        client_ip: IpAddr,
        ua_info: UserAgentInfo,
        status: sys_login_log::LoginStatus,
        fail_reason: Option<String>,
    ) {
        let db = self.db.clone();
        let login_location = self.ip_searcher.search_location(&client_ip);

        tokio::spawn(async move {
            let log: sys_login_log::ActiveModel = CreateLoginLogDto {
                user_id,
                user_name,
                client_ip,
                login_location,
                ua_info,
                status,
                fail_reason,
            }
            .into();

            if let Err(e) = log.insert(&db).await {
                tracing::error!("记录登录日志失败: {}", e);
            }
        });
    }

    /// 获取全部登录日志（管理员）
    pub async fn get_all_login_logs(
        &self,
        query: LoginLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<LoginLogVo>> {
        let mut select = sys_login_log::Entity::find();

        if let Some(user_name) = &query.user_name {
            if !user_name.is_empty() {
                select =
                    select.filter(sys_login_log::Column::UserName.contains(user_name.as_str()));
            }
        }
        if let Some(login_ip) = &query.login_ip {
            if !login_ip.is_empty() {
                select = select.filter(
                    Expr::col((sys_login_log::Entity, sys_login_log::Column::LoginIp))
                        .cast_as(Alias::new("text"))
                        .like(format!("%{}%", login_ip)),
                );
            }
        }
        if let Some(start_time) = query.start_time {
            select = select.filter(sys_login_log::Column::LoginTime.gte(start_time));
        }
        if let Some(end_time) = query.end_time {
            select = select.filter(sys_login_log::Column::LoginTime.lte(end_time));
        }
        if let Some(status) = query.status {
            select = select.filter(sys_login_log::Column::Status.eq(status));
        }

        select = select.order_by_desc(sys_login_log::Column::LoginTime);

        let page = select
            .page(&self.db, &pagination)
            .await
            .context("查询登录日志失败")?;

        Ok(page.map(LoginLogVo::from_model))
    }

    /// 获取用户登录日志
    pub async fn get_user_login_logs(
        &self,
        login_id: &str,
        query: LoginLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<LoginLogVo>> {
        let user_id: i64 = login_id
            .parse()
            .map_err(|_| common::error::ApiErrors::BadRequest("无效的用户ID".to_string()))?;

        let mut select =
            sys_login_log::Entity::find().filter(sys_login_log::Column::UserId.eq(user_id));

        // 时间范围筛选
        if let Some(start_time) = query.start_time {
            select = select.filter(sys_login_log::Column::LoginTime.gte(start_time));
        }
        if let Some(end_time) = query.end_time {
            select = select.filter(sys_login_log::Column::LoginTime.lte(end_time));
        }

        // 状态筛选
        if let Some(status) = query.status {
            select = select.filter(sys_login_log::Column::Status.eq(status));
        }

        // 按登录时间倒序
        select = select.order_by_desc(sys_login_log::Column::LoginTime);

        let page = select
            .page(&self.db, &pagination)
            .await
            .context("查询登录日志失败")?;

        let page = page.map(LoginLogVo::from_model);
        Ok(page)
    }
}
