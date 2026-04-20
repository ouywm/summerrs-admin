//! AI 渠道账号/密钥池表（一个渠道下的实际可调度账号）
//! 对应 sql/ai/channel_account.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashSet;

/// 状态：1=启用 2=禁用 3=额度耗尽 4=过期 5=冷却中
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum ChannelAccountStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 额度耗尽
    #[sea_orm(num_value = 3)]
    QuotaExhausted = 3,
    /// 过期
    #[sea_orm(num_value = 4)]
    Expired = 4,
    /// 冷却中
    #[sea_orm(num_value = 5)]
    CoolingDown = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_account")]
pub struct Model {
    /// 账号ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属渠道ID（ai.channel.id）
    pub channel_id: i64,
    /// 账号名称（便于识别具体 Key/OAuth 账号）
    pub name: String,
    /// 凭证类型：api_key/oauth/cookie/session/token 等
    pub credential_type: String,
    /// 凭证载荷（JSON，如 {"api_key": "..."}、OAuth token、cookie 等）
    #[sea_orm(column_type = "JsonBinary")]
    pub credentials: serde_json::Value,
    /// 外部密钥管理引用（如 Vault/KMS 路径），为空表示直接落库 credentials
    pub secret_ref: String,
    /// 状态：1=启用 2=禁用 3=额度耗尽 4=过期 5=冷却中
    pub status: ChannelAccountStatus,
    /// 当前是否允许被路由器调度
    pub schedulable: bool,
    /// 账号优先级（同渠道内可二次调度）
    pub priority: i32,
    /// 账号权重（同优先级内加权随机）
    pub weight: i32,
    /// 账号级成本倍率快照，可用于不同账号不同采购价
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub rate_multiplier: BigDecimal,
    /// 并发上限（0=不限制）
    pub concurrency_limit: i32,
    /// 账号总额度上限（0=未知/不限制）
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_limit: BigDecimal,
    /// 账号已用额度
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_used: BigDecimal,
    /// 账号级余额快照
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance: BigDecimal,
    /// 账号余额更新时间
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    /// 最近测速响应时间（毫秒）
    pub response_time: i32,
    /// 连续失败次数
    pub failure_streak: i32,
    /// 最近一次实际使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 最近错误时间
    pub last_error_at: Option<DateTimeWithTimeZone>,
    /// 最近错误码
    pub last_error_code: String,
    /// 最近错误摘要
    #[sea_orm(column_type = "Text")]
    pub last_error_message: String,
    /// 速率限制冷却到期时间
    pub rate_limited_until: Option<DateTimeWithTimeZone>,
    /// 上游过载冷却到期时间
    pub overload_until: Option<DateTimeWithTimeZone>,
    /// 账号凭证失效时间
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// 账号级测速模型
    pub test_model: String,
    /// 最近测速时间
    pub test_time: Option<DateTimeWithTimeZone>,
    /// 账号级扩展字段（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub extra: serde_json::Value,
    /// 软删除时间
    pub deleted_at: Option<DateTimeWithTimeZone>,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
    /// 被禁用的 API Key 明细列表（`[{key, disabled_at, error_code, reason}]`）。
    /// 仅在 `credential_type = "api_key"` 且 `credentials.api_keys` 是数组时生效。
    /// 选 key 时会用它去 diff `api_keys()` 得到 `enabled_api_keys()`。
    #[sea_orm(column_type = "JsonBinary", default_value = "[]")]
    pub disabled_api_keys: serde_json::Value,

    /// 关联渠道（多对一，逻辑关联 ai.channel.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "channel_id", to = "id", skip_fk)]
    /// channel
    pub channel: Option<super::channel::Entity>,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}

impl Entity {
    pub async fn find_schedulable_by_channel_ids<C>(
        db: &C,
        channel_ids: &[i64],
    ) -> Result<Vec<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        if channel_ids.is_empty() {
            return Ok(Vec::new());
        }

        Self::find()
            .filter(Column::ChannelId.is_in(channel_ids.to_vec()))
            .filter(Column::DeletedAt.is_null())
            .filter(Column::Schedulable.eq(true))
            .filter(Column::Status.eq(ChannelAccountStatus::Enabled))
            .order_by_asc(Column::ChannelId)
            .order_by_desc(Column::Priority)
            .order_by_desc(Column::Weight)
            .order_by_desc(Column::Id)
            .all(db)
            .await
    }
}

impl Model {
    /// **单 key 快捷方法**，等价于 `enabled_api_keys().into_iter().next()`。
    ///
    /// 保留签名给既有调用方（部分早期代码拿第一个 key 就用）；新代码**不要**直接用，
    /// 走 `KeyPicker` + `enabled_api_keys()`，才能吃到 multi-key 轮询 + disabled-key 过滤。
    pub fn api_key(&self) -> Option<String> {
        self.enabled_api_keys().into_iter().next()
    }

    /// 凭证里的全部 API Key（合并旧 `api_key` + 新 `api_keys`，去重去空）。
    ///
    /// - `credentials.api_key: "sk-x"`（遗留 single-key 格式）
    /// - `credentials.api_keys: ["sk-1", "sk-2"]`（新的 multi-key 格式）
    /// - 两者可共存，按出现顺序拼接
    /// - 去重保持首次出现顺序
    ///
    /// OAuth account 返 `[]`（`is_oauth()` 为 true 时）。
    pub fn api_keys(&self) -> Vec<String> {
        if self.is_oauth() {
            return Vec::new();
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<String> = Vec::new();

        // 旧 api_key
        if let Some(s) = self
            .credentials
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let owned = s.to_owned();
            if seen.insert(owned.clone()) {
                out.push(owned);
            }
        }

        // 新 api_keys 数组
        if let Some(arr) = self.credentials.get("api_keys").and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    let owned = s.to_owned();
                    if seen.insert(owned.clone()) {
                        out.push(owned);
                    }
                }
            }
        }

        out
    }

    /// 被禁用的 API Key 集合（从 `disabled_api_keys` JSONB 数组的 `key` 字段取）。
    pub fn disabled_key_set(&self) -> HashSet<String> {
        let Some(arr) = self.disabled_api_keys.as_array() else {
            return HashSet::new();
        };
        arr.iter()
            .filter_map(|item| item.get("key").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    /// 可用 API Key —— `api_keys()` 去掉 `disabled_key_set()`。
    pub fn enabled_api_keys(&self) -> Vec<String> {
        let disabled = self.disabled_key_set();
        if disabled.is_empty() {
            return self.api_keys();
        }
        self.api_keys()
            .into_iter()
            .filter(|k| !disabled.contains(k))
            .collect()
    }

    /// 是否 OAuth 账号（`credential_type = "oauth"` 或 `credentials.oauth` 对象存在）。
    pub fn is_oauth(&self) -> bool {
        if self.credential_type.eq_ignore_ascii_case("oauth") {
            return true;
        }
        self.credentials
            .get("oauth")
            .and_then(|v| v.as_object())
            .is_some_and(|o| !o.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_account(credentials: serde_json::Value, disabled: serde_json::Value) -> Model {
        Model {
            id: 1,
            channel_id: 1,
            name: "t".into(),
            credential_type: "api_key".into(),
            credentials,
            secret_ref: String::new(),
            status: ChannelAccountStatus::Enabled,
            schedulable: true,
            priority: 0,
            weight: 1,
            rate_multiplier: BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: BigDecimal::from(0),
            quota_used: BigDecimal::from(0),
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: None,
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: String::new(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: String::new(),
            update_time: chrono::Utc::now().fixed_offset(),
            disabled_api_keys: disabled,
        }
    }

    #[test]
    fn api_keys_reads_legacy_single_key_field() {
        let m = mk_account(
            serde_json::json!({"api_key": "sk-legacy"}),
            serde_json::json!([]),
        );
        assert_eq!(m.api_keys(), vec!["sk-legacy"]);
    }

    #[test]
    fn api_keys_reads_new_array_field() {
        let m = mk_account(
            serde_json::json!({"api_keys": ["sk-1", "sk-2", "sk-3"]}),
            serde_json::json!([]),
        );
        assert_eq!(m.api_keys(), vec!["sk-1", "sk-2", "sk-3"]);
    }

    #[test]
    fn api_keys_merges_legacy_and_array_with_dedup() {
        let m = mk_account(
            serde_json::json!({"api_key": "sk-a", "api_keys": ["sk-a", "sk-b", "sk-c"]}),
            serde_json::json!([]),
        );
        assert_eq!(m.api_keys(), vec!["sk-a", "sk-b", "sk-c"]);
    }

    #[test]
    fn api_keys_trims_whitespace_and_ignores_empty() {
        let m = mk_account(
            serde_json::json!({"api_keys": ["  sk-1  ", "", "   ", "sk-2"]}),
            serde_json::json!([]),
        );
        assert_eq!(m.api_keys(), vec!["sk-1", "sk-2"]);
    }

    #[test]
    fn api_keys_empty_when_oauth() {
        let mut m = mk_account(
            serde_json::json!({"api_keys": ["sk-1"]}),
            serde_json::json!([]),
        );
        m.credential_type = "oauth".into();
        assert!(m.api_keys().is_empty());
        assert!(m.is_oauth());
    }

    #[test]
    fn enabled_api_keys_filters_disabled() {
        let m = mk_account(
            serde_json::json!({"api_keys": ["sk-1", "sk-2", "sk-3"]}),
            serde_json::json!([
                {"key": "sk-2", "disabled_at": "2026-01-01T00:00:00Z", "error_code": 401, "reason": "test"}
            ]),
        );
        assert_eq!(m.enabled_api_keys(), vec!["sk-1", "sk-3"]);
    }

    #[test]
    fn disabled_key_set_reads_key_field() {
        let m = mk_account(
            serde_json::json!({"api_keys": ["sk-1"]}),
            serde_json::json!([
                {"key": "sk-x", "disabled_at": "2026-01-01T00:00:00Z", "error_code": 401, "reason": "r"},
                {"key": "sk-y", "disabled_at": "2026-01-02T00:00:00Z", "error_code": 429, "reason": ""}
            ]),
        );
        let set = m.disabled_key_set();
        assert!(set.contains("sk-x"));
        assert!(set.contains("sk-y"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn api_key_returns_first_enabled() {
        let m = mk_account(
            serde_json::json!({"api_keys": ["sk-banned", "sk-ok"]}),
            serde_json::json!([
                {"key": "sk-banned", "disabled_at": "2026-01-01T00:00:00Z", "error_code": 401, "reason": ""}
            ]),
        );
        assert_eq!(m.api_key().as_deref(), Some("sk-ok"));
    }

    #[test]
    fn is_oauth_detects_credentials_oauth_object() {
        let m = mk_account(
            serde_json::json!({"oauth": {"access_token": "at", "refresh_token": "rt", "expires_at": "2026-04-20T12:00:00Z"}}),
            serde_json::json!([]),
        );
        assert!(m.is_oauth());
    }
}
