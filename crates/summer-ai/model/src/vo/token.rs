use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::billing::token::{self, TokenStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenVo {
    pub id: i64,
    pub user_id: i64,
    pub service_account_id: i64,
    pub project_id: i64,
    pub name: String,
    pub key_prefix: String,
    pub status: TokenStatus,
    pub remain_quota: i64,
    pub used_quota: i64,
    pub unlimited_quota: bool,
    pub models: Vec<String>,
    pub endpoint_scopes: Vec<String>,
    pub group_code_override: String,
    pub rpm_limit: i32,
    pub tpm_limit: i64,
    pub concurrency_limit: i32,
    pub daily_quota_limit: i64,
    pub monthly_quota_limit: i64,
    pub daily_used_quota: i64,
    pub monthly_used_quota: i64,
    pub expire_time: Option<DateTimeWithTimeZone>,
    pub access_time: Option<DateTimeWithTimeZone>,
    pub last_used_ip: String,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl TokenVo {
    pub fn from_model(m: token::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            service_account_id: m.service_account_id,
            project_id: m.project_id,
            name: m.name,
            key_prefix: m.key_prefix,
            status: m.status,
            remain_quota: m.remain_quota,
            used_quota: m.used_quota,
            unlimited_quota: m.unlimited_quota,
            models: json_string_array(&m.models),
            endpoint_scopes: json_string_array(&m.endpoint_scopes),
            group_code_override: m.group_code_override,
            rpm_limit: m.rpm_limit,
            tpm_limit: m.tpm_limit,
            concurrency_limit: m.concurrency_limit,
            daily_quota_limit: m.daily_quota_limit,
            monthly_quota_limit: m.monthly_quota_limit,
            daily_used_quota: m.daily_used_quota,
            monthly_used_quota: m.monthly_used_quota,
            expire_time: m.expire_time,
            access_time: m.access_time,
            last_used_ip: m.last_used_ip,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenDetailVo {
    #[serde(flatten)]
    pub base: TokenVo,
    pub ip_whitelist: Vec<String>,
    pub ip_blacklist: Vec<String>,
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    pub last_user_agent: String,
}

impl TokenDetailVo {
    pub fn from_model(m: token::Model) -> Self {
        let ip_whitelist = json_string_array(&m.ip_whitelist);
        let ip_blacklist = json_string_array(&m.ip_blacklist);
        let daily_window_start = m.daily_window_start;
        let monthly_window_start = m.monthly_window_start;
        let last_user_agent = m.last_user_agent.clone();
        Self {
            base: TokenVo::from_model(m),
            ip_whitelist,
            ip_blacklist,
            daily_window_start,
            monthly_window_start,
            last_user_agent,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTokenVo {
    pub token: TokenVo,
    pub raw_key: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RotatedTokenKeyVo {
    pub id: i64,
    pub key_prefix: String,
    pub raw_key: String,
}

fn json_string_array(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
