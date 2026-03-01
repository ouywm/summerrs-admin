use chrono::NaiveDateTime;
use common::user_agent::UserAgentInfo;
use schemars::JsonSchema;
use sea_orm::prelude::IpNetwork;
use sea_orm::Set;
use serde::Deserialize;
use std::net::IpAddr;

use crate::entity::sys_login_log::{self, LoginStatus};

/// 登录日志查询参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginLogQueryDto {
    pub user_name: Option<String>,
    pub login_ip: Option<String>,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub status: Option<LoginStatus>,
}

/// 创建登录日志
pub struct CreateLoginLogDto {
    pub user_id: i64,
    pub user_name: String,
    pub client_ip: IpAddr,
    pub login_location: String,
    pub ua_info: UserAgentInfo,
    pub status: LoginStatus,
    pub fail_reason: Option<String>,
}

impl From<CreateLoginLogDto> for sys_login_log::ActiveModel {
    fn from(dto: CreateLoginLogDto) -> Self {
        Self {
            user_id: Set(dto.user_id),
            user_name: Set(dto.user_name),
            login_ip: Set(IpNetwork::from(dto.client_ip)),
            login_location: Set(dto.login_location),
            user_agent: Set(dto.ua_info.raw),
            browser: Set(dto.ua_info.browser),
            browser_version: Set(dto.ua_info.browser_version),
            os: Set(dto.ua_info.os),
            os_version: Set(dto.ua_info.os_version),
            device: Set(dto.ua_info.device),
            status: Set(dto.status),
            fail_reason: Set(dto.fail_reason.unwrap_or_default()),
            ..Default::default()
        }
    }
}
