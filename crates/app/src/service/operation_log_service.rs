use crate::plugin::background_task::BackgroundTaskQueue;
use crate::plugin::ip2region::Ip2RegionSearcher;
use crate::plugin::log_batch_collector::OperationLogCollector;
use crate::plugin::sea_orm::pagination::{Page, Pagination, PaginationExt};
use crate::plugin::sea_orm::DbConn;
use anyhow::Context;
use common::error::ApiResult;
use model::dto::operation_log::{CreateOperationLogDto, OperationLogQueryDto};
use model::entity::sys_operation_log;
use model::vo::operation_log::{OperationLogDetailVo, OperationLogVo};
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_auth::{LoginId, SessionManager, UserType};
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;
use std::net::IpAddr;

#[derive(Clone, Service)]
pub struct OperationLogService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
    #[inject(component)]
    task_queue: BackgroundTaskQueue,
    #[inject(component)]
    op_collector: OperationLogCollector,
    #[inject(component)]
    auth: SessionManager,
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

        // 提取当前登录用户 ID，未登录时为 0
        let user_id = parts.extensions.get::<LoginId>()
            .map(|lid| lid.user_id)
            .unwrap_or(0);

        // 提取 OperationLogService 组件
        let op_svc = parts.get_component::<OperationLogService>()?;

        Ok(OperationLogContext {
            method,
            uri,
            query,
            user_agent,
            client_ip,
            user_id,
            op_svc,
        })
    }
}

/// OperationLogContext 是内部日志提取器，对 OpenAPI 文档透明（不生成任何参数描述）
impl summer_web::aide::OperationInput for OperationLogContext {}

impl OperationLogService {
    /// 从 session 获取操作人昵称，失败时返回 None
    async fn get_user_name(auth: &SessionManager, user_id: i64) -> Option<String> {
        // 尝试所有用户类型
        for user_type in UserType::all() {
            let login_id = user_type.login_id(user_id);
            if let Ok(Some(session)) = auth.get_session(&login_id).await {
                return Some(session.profile.nick_name().to_string());
            }
        }
        None
    }

    /// 异步记录操作日志（通过后台任务队列预处理，批量收集器写入）
    pub fn record_async(&self, dto: CreateOperationLogDto) {
        let ip_location = self.ip_searcher.search_location(&dto.client_ip);
        let op_collector = self.op_collector.clone();
        let user_id = dto.user_id;
        let auth = self.auth.clone();

        self.task_queue.spawn(async move {
            // 通过 login_id 获取操作人昵称，获取失败（如退出登录后 token 已销毁）时回退为"未知用户"
            let user_name = if user_id > 0 {
                Self::get_user_name(&auth, user_id)
                    .await
                    .or_else(|| Some("未知用户".to_string()))
            } else {
                Some("未知用户".to_string())
            };

            op_collector.push(dto.into_active_model(user_name, ip_location));
        });
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
            .ok_or_else(|| common::error::ApiErrors::NotFound("操作日志不存在".to_string()))?;

        Ok(OperationLogDetailVo::from_model(model))
    }
}
