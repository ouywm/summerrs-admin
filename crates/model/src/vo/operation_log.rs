use chrono::NaiveDateTime;
use common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value as Json;
use std::net::IpAddr;

use crate::entity::sys_operation_log::{BusinessType, Model, OperationStatus};

fn business_type_text(bt: &BusinessType) -> &'static str {
    match bt {
        BusinessType::Other => "其他",
        BusinessType::Create => "新增",
        BusinessType::Update => "修改",
        BusinessType::Delete => "删除",
        BusinessType::Query => "查询",
        BusinessType::Export => "导出",
        BusinessType::Import => "导入",
        BusinessType::Auth => "授权",
    }
}

fn status_text(s: &OperationStatus) -> &'static str {
    match s {
        OperationStatus::Success => "成功",
        OperationStatus::Failed => "失败",
        OperationStatus::Exception => "异常",
    }
}

/// 操作日志列表项（精简字段，用于表格展示）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogVo {
    pub id: i64,
    pub user_name: Option<String>,
    pub module: String,
    pub action: String,
    pub business_type: BusinessType,
    pub business_type_text: String,
    pub request_method: String,
    pub client_ip: Option<IpAddr>,
    pub ip_location: Option<String>,
    pub status: OperationStatus,
    pub status_text: String,
    pub duration: i64,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
}

impl OperationLogVo {
    pub fn from_model(model: Model) -> Self {
        Self {
            id: model.id,
            user_name: model.user_name,
            module: model.module,
            action: model.action,
            business_type_text: business_type_text(&model.business_type).to_string(),
            business_type: model.business_type,
            request_method: model.request_method,
            client_ip: model.client_ip.map(|ip| ip.ip()),
            ip_location: model.ip_location,
            status_text: status_text(&model.status).to_string(),
            status: model.status,
            duration: model.duration,
            create_time: model.create_time,
        }
    }
}

/// 操作日志详情（完整字段）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogDetailVo {
    pub id: i64,
    pub user_id: Option<i64>,
    pub user_name: Option<String>,
    pub module: String,
    pub action: String,
    pub business_type: BusinessType,
    pub business_type_text: String,
    pub request_method: String,
    pub request_url: String,
    pub request_params: Option<Json>,
    pub response_body: Option<Json>,
    pub response_code: i16,
    pub client_ip: Option<IpAddr>,
    pub ip_location: Option<String>,
    pub user_agent: Option<String>,
    pub status: OperationStatus,
    pub status_text: String,
    pub error_msg: Option<String>,
    pub duration: i64,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
}

impl OperationLogDetailVo {
    pub fn from_model(model: Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            user_name: model.user_name,
            module: model.module,
            action: model.action,
            business_type_text: business_type_text(&model.business_type).to_string(),
            business_type: model.business_type,
            request_method: model.request_method,
            request_url: model.request_url,
            request_params: model.request_params,
            response_body: model.response_body,
            response_code: model.response_code,
            client_ip: model.client_ip.map(|ip| ip.ip()),
            ip_location: model.ip_location,
            user_agent: model.user_agent,
            status_text: status_text(&model.status).to_string(),
            status: model.status,
            error_msg: model.error_msg,
            duration: model.duration,
            create_time: model.create_time,
        }
    }
}
