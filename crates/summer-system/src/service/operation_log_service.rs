use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use std::net::IpAddr;
use summer::plugin::Service;
use summer_common::error::ApiResult;
use summer_plugins::ip2region::Ip2RegionSearcher;
use summer_plugins::log_batch_collector::OperationLogCollector;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_system_model::dto::operation_log::{CreateOperationLogDto, OperationLogQueryDto};
use summer_system_model::entity::sys_operation_log;
use summer_system_model::vo::operation_log::{OperationLogDetailVo, OperationLogVo};
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;

#[derive(Clone, Service)]
pub struct OperationLogService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
    #[inject(component)]
    op_collector: OperationLogCollector,
}

/// 操作日志上下文提取器
///
/// 合并 Method、Uri、HeaderMap、ClientIp、LoginId、OperationLogService
/// 为单一提取器，避免注入过多参数导致 axum Handler trait 不满足。
pub struct OperationLogContext {
    pub method: String,
    pub uri: String,
    pub query: Option<String>,
    pub user_agent: Option<String>,
    pub client_ip: IpAddr,
    pub user_id: i64,
    /// 操作人昵称（从当前请求的 UserSession 中提取，未登录时为 None）
    pub nick_name: Option<String>,
    pub op_svc: OperationLogService,
}

impl<S: Send + Sync> FromRequestParts<S> for OperationLogContext {
    type Rejection = summer_web::error::WebError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let method = parts.method.to_string();
        let query = parts.uri.query().map(|q| q.to_string());
        let uri = parts.uri.to_string();
        let user_agent = parts
            .headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // 提取客户端 IP，失败时使用 127.0.0.1
        let client_ip = axum_client_ip::ClientIp::from_request_parts(parts, _state)
            .await
            .map(|axum_client_ip::ClientIp(ip)| ip)
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        // 从 UserSession extension 中提取用户 ID 和昵称（JWT 中已包含，零 IO）
        let (user_id, nick_name) = parts
            .extensions
            .get::<summer_auth::UserSession>()
            .map(|s| (s.login_id.user_id, Some(s.profile.nick_name().to_string())))
            .unwrap_or((0, None));

        // 提取 OperationLogService 组件
        let op_svc = parts.get_component::<OperationLogService>()?;

        Ok(OperationLogContext {
            method,
            uri,
            query,
            user_agent,
            client_ip,
            user_id,
            nick_name,
            op_svc,
        })
    }
}

/// OperationLogContext 是内部日志提取器，对 OpenAPI 文档透明（不生成任何参数描述）
impl summer_web::aide::OperationInput for OperationLogContext {}

impl OperationLogService {
    /// 异步记录操作日志（直接入批量收集器）
    pub fn record_async(&self, dto: CreateOperationLogDto, nick_name: Option<String>) {
        let ip_location = self.ip_searcher.search_location(&dto.client_ip);
        let user_name = nick_name.or_else(|| Some("未知用户".to_string()));
        if let Err(error) = self
            .op_collector
            .push(dto.into_active_model(user_name, ip_location))
        {
            tracing::warn!("操作日志批量入队失败: {:?}", error);
        }
    }

    /// 查询操作日志（分页 + 条件筛选）
    pub async fn get_operation_logs(
        &self,
        query: OperationLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<OperationLogVo>> {
        let mut select = sys_operation_log::Entity::find().filter(query);

        select = select.order_by_desc(sys_operation_log::Column::CreateTime);

        let page = select
            .page(&self.db, &pagination)
            .await
            .context("查询操作日志失败")?;

        Ok(page.map(OperationLogVo::from_model))
    }

    /// 查询操作日志详情
    pub async fn get_operation_log_detail(&self, id: i64) -> ApiResult<OperationLogDetailVo> {
        let model = sys_operation_log::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询操作日志详情失败")?
            .ok_or_else(|| {
                summer_common::error::ApiErrors::NotFound("操作日志不存在".to_string())
            })?;

        Ok(OperationLogDetailVo::from_model(model))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn operation_log_service_pushes_directly_to_collector() {
        let source = include_str!("operation_log_service.rs");
        let prod_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(!prod_source.contains("task_queue: BackgroundTaskQueue"));
        assert!(!prod_source.contains("self.task_queue.spawn"));
        assert!(prod_source.contains(".op_collector"));
        assert!(prod_source.contains(".push(dto.into_active_model"));
    }
}
