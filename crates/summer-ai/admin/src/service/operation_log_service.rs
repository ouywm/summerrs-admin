use std::net::IpAddr;

use summer::plugin::Service;
use summer_common::user_agent::UserAgentInfo;
use summer_plugins::ip2region::Ip2RegionSearcher;
use summer_plugins::log_batch_collector::OperationLogCollector;
use summer_system_model::dto::operation_log::CreateOperationLogDto;
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;

#[derive(Clone, Service)]
pub struct OperationLogService {
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
    #[inject(component)]
    op_collector: OperationLogCollector,
}

/// 操作日志上下文提取器。
///
/// `#[log]` 过程宏会把多个请求级依赖折叠成这个提取器，避免 handler 参数过多。
pub struct OperationLogContext {
    pub method: String,
    pub uri: String,
    pub query: Option<String>,
    pub user_agent: Option<String>,
    pub client_ip: IpAddr,
    pub user_id: i64,
    pub nick_name: Option<String>,
    pub op_svc: OperationLogService,
}

impl<S: Send + Sync> FromRequestParts<S> for OperationLogContext {
    type Rejection = summer_web::error::WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let method = parts.method.to_string();
        let query = parts.uri.query().map(|query| query.to_string());
        let uri = parts.uri.to_string();
        let user_agent = UserAgentInfo::raw_optional_from_headers(&parts.headers);

        let client_ip = axum_client_ip::ClientIp::from_request_parts(parts, state)
            .await
            .map(|axum_client_ip::ClientIp(ip)| ip)
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        let (user_id, nick_name) = parts
            .extensions
            .get::<summer_auth::UserSession>()
            .map(|session| {
                (
                    session.login_id.user_id,
                    Some(session.profile.nick_name().to_string()),
                )
            })
            .unwrap_or((0, None));

        let op_svc = parts.get_component::<OperationLogService>()?;

        Ok(Self {
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

impl summer_web::aide::OperationInput for OperationLogContext {}

impl OperationLogService {
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
}
