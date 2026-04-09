use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use std::net::IpAddr;
use summer::plugin::Service;
use summer_auth::LoginId;
use summer_common::error::ApiResult;
use summer_common::user_agent::UserAgentInfo;
use summer_plugins::ip2region::Ip2RegionSearcher;
use summer_plugins::log_batch_collector::LoginLogCollector;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_system_model::dto::login_log::{CreateLoginLogDto, LoginLogQueryDto};
use summer_system_model::entity::sys_login_log;
use summer_system_model::vo::login_log::LoginLogVo;

#[derive(Clone, Service)]
pub struct LoginLogService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
    #[inject(component)]
    login_collector: LoginLogCollector,
}

impl LoginLogService {
    /// 记录登录日志（直接入批量收集器）
    pub fn record_login_async(
        &self,
        user_id: i64,
        user_name: String,
        client_ip: IpAddr,
        ua_info: UserAgentInfo,
        status: sys_login_log::LoginStatus,
        fail_reason: Option<String>,
    ) {
        let login_location = self.ip_searcher.search_location(&client_ip);
        let mut log: sys_login_log::ActiveModel = CreateLoginLogDto {
            user_id,
            user_name,
            client_ip,
            login_location,
            ua_info,
            status,
            fail_reason,
        }
        .into();

        // insert_many 不触发 before_save，手动设置时间戳
        let now = chrono::Local::now().naive_local();
        log.create_time = Set(now);
        if log.login_time.is_not_set() {
            log.login_time = Set(now);
        }

        if let Err(error) = self.login_collector.push(log) {
            tracing::warn!("登录日志批量入队失败: {:?}", error);
        }
    }

    /// 获取全部登录日志（管理员）
    pub async fn get_all_login_logs(
        &self,
        query: LoginLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<LoginLogVo>> {
        let page = sys_login_log::Entity::find()
            .filter(query)
            .order_by_desc(sys_login_log::Column::LoginTime)
            .page(&self.db, &pagination)
            .await
            .context("查询登录日志失败")?;

        Ok(page.map(LoginLogVo::from_model))
    }

    /// 获取用户登录日志
    pub async fn get_user_login_logs(
        &self,
        login_id: &LoginId,
        query: LoginLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<LoginLogVo>> {
        let user_id = login_id.user_id;

        let page = sys_login_log::Entity::find()
            .filter(sys_login_log::Column::UserId.eq(user_id))
            .filter(query)
            .order_by_desc(sys_login_log::Column::LoginTime)
            .page(&self.db, &pagination)
            .await
            .context("查询登录日志失败")?;

        Ok(page.map(LoginLogVo::from_model))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn login_log_service_pushes_directly_to_collector() {
        let source = include_str!("login_log_service.rs");
        let prod_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(!prod_source.contains("task_queue: BackgroundTaskQueue"));
        assert!(!prod_source.contains("self.task_queue.spawn"));
        assert!(prod_source.contains("self.login_collector.push"));
    }
}
