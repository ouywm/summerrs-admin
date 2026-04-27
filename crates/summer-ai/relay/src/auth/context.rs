//! 鉴权通过后挂到 `Request::extensions` 里的 token 上下文。
//!
//! 供下游 handler / service / 后续 Log / Billing 从同一条请求中读取"调用方是谁、
//! 有什么配额、属于哪个项目"，不必每个 handler 再查一遍库。
//!
//! 字段是 `ai.token` 表的子集——只拷贝下游会用到的，其他（rpm/tpm/ip 白名单等）
//! 由各自的中间件自己查。

use summer_ai_model::entity::billing::token;

/// 鉴权通过后注入到 `Request::extensions` 的 token 上下文。
#[derive(Debug, Clone)]
pub struct AiTokenContext {
    pub token_id: i64,
    pub user_id: i64,
    pub project_id: i64,
    pub service_account_id: i64,
    pub token_name: String,
    /// 仅日志 / 展示，不参与校验。
    pub key_prefix: String,
    pub unlimited_quota: bool,
    /// 剩余配额（下游 Billing reserve 用）。
    pub remain_quota: i64,
    /// 令牌级分组覆盖（空字符串表示跟随 user_quota.channel_group）。
    pub group_code_override: String,
    /// 允许使用的模型白名单。本阶段只保存，不校验。
    pub allowed_models: Vec<String>,
}

impl AiTokenContext {
    /// 从 `ai.token::Model` 抽取下游关心的字段。
    pub fn from_model(m: &token::Model) -> Self {
        Self {
            token_id: m.id,
            user_id: m.user_id,
            project_id: m.project_id,
            service_account_id: m.service_account_id,
            token_name: m.name.clone(),
            key_prefix: m.key_prefix.clone(),
            unlimited_quota: m.unlimited_quota,
            remain_quota: m.remain_quota,
            group_code_override: m.group_code_override.clone(),
            allowed_models: json_string_array(&m.models),
        }
    }
}

fn json_string_array(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::prelude::BigDecimal;

    fn mk_model(unlimited: bool, remain: i64) -> token::Model {
        token::Model {
            id: 42,
            user_id: 1,
            service_account_id: 0,
            project_id: 7,
            name: "dev".into(),
            key_hash: "abc".into(),
            key_prefix: "sk-test".into(),
            status: token::TokenStatus::Enabled,
            remain_quota: remain,
            used_quota: 0,
            unlimited_quota: unlimited,
            models: serde_json::json!(["gpt-4o-mini", "claude-sonnet"]),
            endpoint_scopes: serde_json::json!(["chat"]),
            ip_whitelist: serde_json::json!([]),
            ip_blacklist: serde_json::json!([]),
            group_code_override: "vip".into(),
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            daily_quota_limit: 0,
            monthly_quota_limit: 0,
            daily_used_quota: 0,
            monthly_used_quota: 0,
            daily_window_start: None,
            monthly_window_start: None,
            expire_time: None,
            access_time: None,
            last_used_ip: String::new(),
            last_user_agent: String::new(),
            remark: String::new(),
            create_by: String::new(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: String::new(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    // 只消一下 BigDecimal 未使用告警（token::Model 不含 BigDecimal，但测试里其他 model
    // 将来可能用到；先留着占位）。
    #[allow(dead_code)]
    fn _compiles(_b: BigDecimal) {}

    #[test]
    fn from_model_extracts_expected_fields() {
        let m = mk_model(true, 1000);
        let ctx = AiTokenContext::from_model(&m);
        assert_eq!(ctx.token_id, 42);
        assert_eq!(ctx.user_id, 1);
        assert_eq!(ctx.project_id, 7);
        assert!(ctx.unlimited_quota);
        assert_eq!(ctx.remain_quota, 1000);
        assert_eq!(ctx.group_code_override, "vip");
        assert_eq!(ctx.allowed_models, vec!["gpt-4o-mini", "claude-sonnet"]);
        assert_eq!(ctx.key_prefix, "sk-test");
    }

    #[test]
    fn json_string_array_empty_on_non_array() {
        assert!(json_string_array(&serde_json::json!({})).is_empty());
        assert!(json_string_array(&serde_json::Value::Null).is_empty());
    }
}
