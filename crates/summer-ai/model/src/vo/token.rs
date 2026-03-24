use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::token::{self, TokenStatus};

/// 令牌 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenVo {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub key_prefix: String,
    pub status: TokenStatus,
    pub remain_quota: i64,
    pub used_quota: i64,
    pub unlimited_quota: bool,
    pub models: serde_json::Value,
    pub group_code_override: String,
    pub rpm_limit: i32,
    pub tpm_limit: i64,
    pub concurrency_limit: i32,
    pub expire_time: Option<DateTime<FixedOffset>>,
    pub access_time: Option<DateTime<FixedOffset>>,
    pub last_used_ip: String,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
}

impl TokenVo {
    pub fn from_model(m: token::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            name: m.name,
            key_prefix: m.key_prefix,
            status: m.status,
            remain_quota: m.remain_quota,
            used_quota: m.used_quota,
            unlimited_quota: m.unlimited_quota,
            models: m.models,
            group_code_override: m.group_code_override,
            rpm_limit: m.rpm_limit,
            tpm_limit: m.tpm_limit,
            concurrency_limit: m.concurrency_limit,
            expire_time: m.expire_time,
            access_time: m.access_time,
            last_used_ip: m.last_used_ip,
            remark: m.remark,
            create_time: m.create_time,
        }
    }
}

/// 创建令牌时返回（含明文 key，仅此一次）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenCreatedVo {
    /// 明文 API Key，仅创建时返回
    pub key: String,
    #[serde(flatten)]
    pub token: TokenVo,
}
