use chrono::NaiveDateTime;
use schemars::JsonSchema;
use sea_orm::prelude::IpNetwork;
use sea_orm::sea_query::{Alias, Expr};
use sea_orm::{ColumnTrait, Condition, ExprTrait, Set};
use serde::Deserialize;
use std::net::IpAddr;
use summer_common::user_agent::UserAgentInfo;

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

impl From<LoginLogQueryDto> for Condition {
    fn from(query: LoginLogQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(user_name) = query.user_name
            && !user_name.is_empty()
        {
            cond = cond.add(sys_login_log::Column::UserName.contains(user_name));
        }
        // 特殊处理：IP 字段需要 cast 为 text
        if let Some(login_ip) = query.login_ip
            && !login_ip.is_empty()
        {
            cond = cond.add(
                Expr::col((sys_login_log::Entity, sys_login_log::Column::LoginIp))
                    .cast_as(Alias::new("text"))
                    .like(format!("%{}%", login_ip)),
            );
        }
        if let Some(start) = query.start_time {
            cond = cond.add(sys_login_log::Column::LoginTime.gte(start));
        }
        if let Some(end) = query.end_time {
            cond = cond.add(sys_login_log::Column::LoginTime.lte(end));
        }
        if let Some(status) = query.status {
            cond = cond.add(sys_login_log::Column::Status.eq(status));
        }
        cond
    }
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
