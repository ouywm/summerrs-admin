use crate::plugin::background_task::BackgroundTaskQueue;
use crate::plugin::ip2region_plugin::Ip2RegionSearcher;
use crate::plugin::log_batch_collector::OperationLogCollector;
use crate::plugin::sea_orm_plugin::DbConn;
use model::dto::operation_log::CreateOperationLogDto;
use model::entity::sys_operation_log;
use sea_orm::prelude::IpNetwork;
use sea_orm::Set;
use spring::plugin::Service;
use spring_sa_token::StpUtil;
use spring_web::axum::extract::FromRequestParts;
use spring_web::axum::http::request::Parts;
use spring_web::extractor::RequestPartsExt;
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
    type Rejection = spring_web::error::WebError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
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
        let user_id = spring_sa_token::LoginIdExtractor::from_request_parts(parts, _state)
            .await
            .map(|ext| ext.0.parse::<i64>().unwrap_or(0))
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

impl OperationLogService {
    /// 从 JWT extra data 获取操作人昵称，失败时返回 None
    async fn get_user_name(login_id: &str) -> Option<String> {
        let token = StpUtil::get_token_by_login_id(login_id).await.ok()?;
        let extra = StpUtil::get_extra_data(&token).await.ok()??;
        extra
            .get("nick_name")
            .and_then(|n| n.as_str())
            .map(String::from)
    }

    /// 异步记录操作日志（通过后台任务队列预处理，批量收集器写入）
    pub fn record_async(&self, dto: CreateOperationLogDto) {
        let ip_location = self.ip_searcher.search_location(&dto.client_ip);
        let op_collector = self.op_collector.clone();

        self.task_queue.spawn(async move {
            // 通过 login_id 获取操作人昵称
            let user_name = if dto.user_id > 0 {
                Self::get_user_name(&dto.user_id.to_string()).await
            } else {
                None
            };

            let model = sys_operation_log::ActiveModel {
                user_id: Set(if dto.user_id > 0 {
                    Some(dto.user_id)
                } else {
                    None
                }),
                user_name: Set(user_name),
                module: Set(dto.module),
                action: Set(dto.action),
                business_type: Set(dto.business_type),
                request_method: Set(dto.request_method),
                request_url: Set(dto.request_url),
                request_params: Set(dto.request_params),
                response_body: Set(dto.response_body),
                response_code: Set(dto.response_code),
                client_ip: Set(Some(IpNetwork::from(dto.client_ip))),
                ip_location: Set(Some(ip_location)),
                user_agent: Set(dto.user_agent),
                status: Set(dto.status),
                error_msg: Set(dto.error_msg),
                duration: Set(dto.duration),
                // insert_many 不触发 before_save，手动设置时间戳
                create_time: Set(chrono::Local::now().naive_local()),
                ..Default::default()
            };

            op_collector.push(model);
        });
    }
}