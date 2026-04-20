//! 单次 relay 请求的生命周期对象。
//!
//! handler 在入口构造 [`RelayContext`]，沿调用链**边走边填**字段：
//!
//! ```text
//!   begin()           — HTTP 解包后，生成 request_id、记录 endpoint/token/IP
//!   attach_channel()  — ChannelStore::pick 之后，填 channel/account 快照
//!   attach_upstream() — invoke_* 之前可选，填 URL/headers 摘要
//!   finalize()        — 成功/失败结束，打上 upstream_status/error/first_byte_at
//! ```
//!
//! 整个对象 `Clone` 便宜（字段多是 `i64` / `String`，`BigDecimal` 一个），可以
//! 直接 clone 进 `tokio::spawn` 的 tracking / billing 后台任务，不加锁。
//!
//! # 为什么不用 `Arc<RwLock<Inner>>`
//!
//! 前中后三段几乎没有共享修改——handler 同步填完 channel/account 后就不再改，
//! 剩下的 `finalize` 在 handler 返回前一次性把 outcome 塞进 clone 好的副本。
//! 加锁只会把同步代码改成异步污染，没有收益。

use std::time::Instant;

use rust_decimal::Decimal;
use summer_ai_core::{AdapterKind, CostProfile};
use summer_ai_model::entity::channels::{channel, channel_account};

use crate::auth::AiTokenContext;
use crate::convert::ingress::IngressFormat;

// ---------------------------------------------------------------------------
// ChannelSnapshot —— 选路命中后从 DB Model 提炼的"够用就行"视图
// ---------------------------------------------------------------------------

/// 命中 channel 的关键字段快照（避免整个 `channel::Model` clone 进 spawn）。
#[derive(Debug, Clone)]
pub struct ChannelSnapshot {
    pub id: i64,
    pub name: String,
    pub vendor_code: String,
    pub channel_group: String,
}

impl ChannelSnapshot {
    pub fn from_model(m: &channel::Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            vendor_code: m.vendor_code.clone(),
            channel_group: m.channel_group.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AccountSnapshot —— 同上，account 字段快照
// ---------------------------------------------------------------------------

/// 命中 channel_account 的关键字段快照。
///
/// `rate_multiplier` 是 account 级成本倍率（不同账号不同采购价）——billing settle
/// 算账时需要。
#[derive(Debug, Clone)]
pub struct AccountSnapshot {
    pub id: i64,
    pub name: String,
    pub rate_multiplier: Decimal,
    /// 选中的 API key 前缀（仅日志 / 展示，不落库全 key）。
    pub key_prefix: String,
}

impl AccountSnapshot {
    pub fn from_model(m: &channel_account::Model, selected_key: &str) -> Self {
        // BigDecimal -> Decimal：走字符串中转一趟（1.0/2.0 之类的小位数足够）
        // sea-orm BigDecimal 转 rust_decimal 没有直接 API，字符串是最稳的桥梁。
        let rate_multiplier =
            Decimal::from_str_exact(&m.rate_multiplier.to_string()).unwrap_or(Decimal::ONE);
        let prefix_len = selected_key.len().min(8);
        Self {
            id: m.id,
            name: m.name.clone(),
            rate_multiplier,
            key_prefix: selected_key[..prefix_len].to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// RelayContext —— 主对象
// ---------------------------------------------------------------------------

/// 一次 relay 请求的"在飞"状态。
///
/// **构造流程**：
///
/// 1. `RelayContext::begin(token, endpoint, format, logical_model, is_stream, client_ip, user_agent)`
///    在 handler 最开头调用——生成 `request_id` 并记录起始时间。
/// 2. `attach_channel(&channel, &account, adapter_kind, cost_profile, actual_model)`
///    在 `ChannelStore::pick` 之后调用——填入选路结果。
/// 3. `mark_first_byte()` / `set_upstream(...)` / `finalize_*()` 按需调用。
#[derive(Debug, Clone)]
pub struct RelayContext {
    // ─── 起始就填好的字段 ───
    pub request_id: String,
    pub token: AiTokenContext,
    pub endpoint: String,
    pub format: IngressFormat,
    pub logical_model: String,
    pub is_stream: bool,
    pub client_ip: String,
    pub user_agent: String,
    pub started_at: Instant,

    // ─── 选路后填入 ───
    pub channel: Option<ChannelSnapshot>,
    pub account: Option<AccountSnapshot>,
    pub adapter_kind: Option<AdapterKind>,
    pub actual_model: Option<String>,
    pub cost_profile: Option<CostProfile>,

    // ─── 发送 / 响应后填入 ───
    pub first_byte_at: Option<Instant>,
    pub upstream_status: Option<u16>,
    pub upstream_request_id: Option<String>,
}

impl RelayContext {
    pub fn begin(
        token: AiTokenContext,
        endpoint: impl Into<String>,
        format: IngressFormat,
        logical_model: impl Into<String>,
        is_stream: bool,
        client_ip: impl Into<String>,
        user_agent: impl Into<String>,
    ) -> Self {
        Self {
            request_id: new_request_id(),
            token,
            endpoint: endpoint.into(),
            format,
            logical_model: logical_model.into(),
            is_stream,
            client_ip: client_ip.into(),
            user_agent: user_agent.into(),
            started_at: Instant::now(),
            channel: None,
            account: None,
            adapter_kind: None,
            actual_model: None,
            cost_profile: None,
            first_byte_at: None,
            upstream_status: None,
            upstream_request_id: None,
        }
    }

    pub fn attach_channel(
        &mut self,
        channel: &channel::Model,
        account: &channel_account::Model,
        selected_key: &str,
        adapter_kind: AdapterKind,
        cost_profile: CostProfile,
        actual_model: impl Into<String>,
    ) {
        self.channel = Some(ChannelSnapshot::from_model(channel));
        self.account = Some(AccountSnapshot::from_model(account, selected_key));
        self.adapter_kind = Some(adapter_kind);
        self.cost_profile = Some(cost_profile);
        self.actual_model = Some(actual_model.into());
    }

    pub fn mark_first_byte(&mut self) {
        if self.first_byte_at.is_none() {
            self.first_byte_at = Some(Instant::now());
        }
    }

    pub fn set_upstream(&mut self, status: u16, upstream_request_id: Option<String>) {
        self.upstream_status = Some(status);
        if let Some(id) = upstream_request_id {
            self.upstream_request_id = Some(id);
        }
    }

    /// 总耗时（毫秒）——多次调用安全（每次算到调用时刻）。
    pub fn elapsed_ms(&self) -> i32 {
        self.started_at.elapsed().as_millis().min(i32::MAX as u128) as i32
    }

    /// 首 token 延迟（毫秒），未开流 / 未拿到首字节时 0。
    pub fn first_token_ms(&self) -> i32 {
        match self.first_byte_at {
            Some(t) => {
                let dur = t.saturating_duration_since(self.started_at);
                dur.as_millis().min(i32::MAX as u128) as i32
            }
            None => 0,
        }
    }

    /// 便利访问 —— channel id / account id，常用于日志。
    pub fn channel_id(&self) -> i64 {
        self.channel.as_ref().map(|c| c.id).unwrap_or(0)
    }
    pub fn account_id(&self) -> i64 {
        self.account.as_ref().map(|a| a.id).unwrap_or(0)
    }
    pub fn actual_model_str(&self) -> &str {
        self.actual_model.as_deref().unwrap_or("")
    }
}

// ---------------------------------------------------------------------------
// request_id 生成
// ---------------------------------------------------------------------------

/// 生成一个请求级唯一 id。用 `chrono` 毫秒时间戳 + 16 位随机后缀。
///
/// 没用 uuid crate：workspace 不强制引，这里只要**有序 + 足够唯一**。时间前缀
/// 让 DB 按 request_id 排序近似按时间排，对审计有帮助。
fn new_request_id() -> String {
    use rand::RngExt;
    let ts = chrono::Utc::now().timestamp_millis();
    let rnd: u64 = rand::rng().random();
    format!("req_{ts:013x}{rnd:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_token() -> AiTokenContext {
        AiTokenContext {
            token_id: 1,
            user_id: 2,
            project_id: 3,
            service_account_id: 0,
            token_name: "dev".into(),
            key_prefix: "sk-dev".into(),
            unlimited_quota: false,
            remain_quota: 1000,
            group_code_override: String::new(),
            allowed_models: Vec::new(),
            allowed_endpoint_scopes: Vec::new(),
        }
    }

    #[test]
    fn request_id_prefix_and_length() {
        let id = new_request_id();
        assert!(id.starts_with("req_"));
        // "req_" + 13 hex (ts) + 16 hex (rnd) = 33 chars
        assert_eq!(id.len(), 4 + 13 + 16);
    }

    #[test]
    fn two_request_ids_are_distinct() {
        let a = new_request_id();
        let b = new_request_id();
        assert_ne!(a, b);
    }

    #[test]
    fn begin_sets_initial_state() {
        let ctx = RelayContext::begin(
            mk_token(),
            "/v1/chat/completions",
            IngressFormat::OpenAI,
            "gpt-4o-mini",
            false,
            "127.0.0.1",
            "curl/8.0",
        );
        assert_eq!(ctx.endpoint, "/v1/chat/completions");
        assert_eq!(ctx.format, IngressFormat::OpenAI);
        assert_eq!(ctx.logical_model, "gpt-4o-mini");
        assert!(!ctx.is_stream);
        assert_eq!(ctx.client_ip, "127.0.0.1");
        assert!(ctx.channel.is_none() && ctx.account.is_none());
        assert_eq!(ctx.channel_id(), 0);
        assert_eq!(ctx.first_token_ms(), 0);
    }

    #[test]
    fn mark_first_byte_is_idempotent() {
        let mut ctx = RelayContext::begin(
            mk_token(),
            "/v1/messages",
            IngressFormat::Claude,
            "claude-3-5-sonnet",
            true,
            "",
            "",
        );
        ctx.mark_first_byte();
        let first = ctx.first_byte_at;
        ctx.mark_first_byte();
        assert_eq!(ctx.first_byte_at, first, "second call must not overwrite");
    }
}
