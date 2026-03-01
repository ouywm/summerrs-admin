use common::serde_utils::datetime_format;
use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use std::net::IpAddr;

use crate::entity::sys_login_log::{LoginStatus, Model};

/// 登录日志响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginLogVo {
    pub id: i64,
    pub user_id: i64,
    pub user_name: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub login_time: NaiveDateTime,
    pub login_ip: IpAddr,
    pub login_location: String,
    pub user_agent: String,
    pub browser: String,
    pub browser_version: String,
    pub os: String,
    pub os_version: String,
    pub device: String,
    pub status: LoginStatus,
    pub status_text: String,
    pub fail_reason: String,
}

impl LoginLogVo {
    pub fn from_model(model: Model) -> Self {
        let status_text = match model.status {
            LoginStatus::Success => "登录成功",
            LoginStatus::Failed => "登录失败",
        };

        Self {
            id: model.id,
            user_id: model.user_id,
            user_name: model.user_name,
            login_time: model.login_time,
            login_ip: model.login_ip.ip(),
            login_location: model.login_location,
            user_agent: model.user_agent,
            browser: model.browser,
            browser_version: model.browser_version,
            os: model.os,
            os_version: model.os_version,
            device: model.device,
            status: model.status,
            status_text: status_text.to_string(),
            fail_reason: model.fail_reason,
        }
    }
}
