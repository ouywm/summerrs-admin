use std::net::IpAddr;

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::entity::sys_operation_log::{BusinessType, OperationStatus};

/// 操作日志查询参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogQueryDto {
    pub user_name: Option<String>,
    pub module: Option<String>,
    pub business_type: Option<BusinessType>,
    pub status: Option<OperationStatus>,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
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
