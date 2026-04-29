use schemars::JsonSchema;
use sea_orm::prelude::IpNetwork;
use sea_orm::sea_query::{Alias, Expr};
use sea_orm::{ColumnTrait, Condition, ExprTrait, NotSet, Set};
use serde::Deserialize;
use std::net::IpAddr;

use crate::entity::sys_operation_log::{self, BusinessType, OperationStatus};

/// 操作日志查询参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogQueryDto {
    pub user_name: Option<String>,
    pub module: Option<String>,
    pub action: Option<String>,
    pub business_type: Option<BusinessType>,
    pub request_method: Option<String>,
    pub request_url: Option<String>,
    pub client_ip: Option<String>,
    pub response_code: Option<i16>,
    pub status: Option<OperationStatus>,
    pub start_time: Option<chrono::NaiveDateTime>,
    pub end_time: Option<chrono::NaiveDateTime>,
}

impl From<OperationLogQueryDto> for Condition {
    fn from(query: OperationLogQueryDto) -> Self {
        let mut cond = Condition::all();

        // 空字符串检查
        if let Some(user_name) = query.user_name
            && !user_name.is_empty()
        {
            cond = cond.add(sys_operation_log::Column::UserName.contains(user_name));
        }
        if let Some(module) = query.module
            && !module.is_empty()
        {
            cond = cond.add(sys_operation_log::Column::Module.contains(module));
        }
        if let Some(action) = query.action
            && !action.is_empty()
        {
            cond = cond.add(sys_operation_log::Column::Action.contains(action));
        }
        if let Some(business_type) = query.business_type {
            cond = cond.add(sys_operation_log::Column::BusinessType.eq(business_type));
        }
        if let Some(request_method) = query.request_method
            && !request_method.is_empty()
        {
            cond = cond.add(sys_operation_log::Column::RequestMethod.eq(request_method));
        }
        if let Some(request_url) = query.request_url
            && !request_url.is_empty()
        {
            cond = cond.add(sys_operation_log::Column::RequestUrl.contains(request_url));
        }
        // 特殊处理：IP 字段需要 cast 为 text
        if let Some(client_ip) = query.client_ip
            && !client_ip.is_empty()
        {
            cond = cond.add(
                Expr::col((
                    sys_operation_log::Entity,
                    sys_operation_log::Column::ClientIp,
                ))
                .cast_as(Alias::new("text"))
                .like(format!("%{}%", client_ip)),
            );
        }
        if let Some(response_code) = query.response_code {
            cond = cond.add(sys_operation_log::Column::ResponseCode.eq(response_code));
        }
        if let Some(status) = query.status {
            cond = cond.add(sys_operation_log::Column::Status.eq(status));
        }
        if let Some(start) = query.start_time {
            cond = cond.add(sys_operation_log::Column::CreateTime.gte(start));
        }
        if let Some(end) = query.end_time {
            cond = cond.add(sys_operation_log::Column::CreateTime.lte(end));
        }
        cond
    }
}

/// 创建操作日志 DTO（由 #[log] 宏生成的代码使用）
pub struct CreateOperationLogDto {
    pub user_id: i64,
    pub module: String,
    pub action: String,
    pub business_type: BusinessType,
    pub request_method: String,
    pub request_url: String,
    pub request_params: Option<serde_json::Value>,
    pub response_body: Option<serde_json::Value>,
    pub response_code: i16,
    pub client_ip: IpAddr,
    pub user_agent: Option<String>,
    pub status: OperationStatus,
    pub error_msg: Option<String>,
    pub duration: i64,
}

impl CreateOperationLogDto {
    /// 转换为 ActiveModel，附加预处理字段（user_name, ip_location, create_time）
    pub fn into_active_model(
        self,
        user_name: Option<String>,
        ip_location: String,
    ) -> sys_operation_log::ActiveModel {
        sys_operation_log::ActiveModel {
            id: NotSet,
            user_id: Set(if self.user_id > 0 {
                Some(self.user_id)
            } else {
                None
            }),
            user_name: Set(user_name),
            module: Set(self.module),
            action: Set(self.action),
            business_type: Set(self.business_type),
            request_method: Set(self.request_method),
            request_url: Set(self.request_url),
            request_params: Set(self.request_params),
            response_body: Set(self.response_body),
            response_code: Set(self.response_code),
            client_ip: Set(Some(IpNetwork::from(self.client_ip))),
            ip_location: Set(Some(ip_location)),
            user_agent: Set(self.user_agent),
            status: Set(self.status),
            error_msg: Set(self.error_msg),
            duration: Set(self.duration),
            create_time: Set(chrono::Local::now().naive_local()),
        }
    }
}
