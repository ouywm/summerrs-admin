//! Provider-agnostic 的请求级选项枚举。
//!
//! 过去用 `Option<String>` 宽松透传上游字段（`reasoning_effort` / `service_tier` 等）
//! —— 字符串在序列化层零成本，但 billing / adapter 里按值分档时需要到处写
//! `as_deref() == Some("high")` 这类魔法字符串，既易写错也不利于 IDE 补全。
//! 这里统一强类型化，配套手写的 serde 实现同时接受字符串和（对 `ReasoningEffort`）
//! 数字 budget 形态，旧 JSON 数据继续可用。

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

// ---------------------------------------------------------------------------
// ReasoningEffort
// ---------------------------------------------------------------------------

/// 推理强度提示。
///
/// Wire 形态：
/// - `None` / `Minimal` / `Low` / `Medium` / `High` / `XHigh` / `Max` → 小写字符串
/// - `Budget(n)` → 数字 `n`（对应 Anthropic `thinking.budget_tokens` 或 Gemini
///   `thinkingConfig.thinkingBudget`）
///
/// Provider 映射（由 adapter 负责）：
/// - OpenAI / o-series：只认 `minimal` / `low` / `medium` / `high`；`XHigh` / `Max`
///   fallback 到 `high`，`Budget(n)` 按阈值近似映射成关键字。
/// - Anthropic：把 `Budget(n)` 直接作为 `thinking.budget_tokens`，关键字按阈值
///   （low ~1024 / medium ~4096 / high ~16384）回填 budget。
/// - Gemini：`thinkingConfig.thinkingBudget` 同理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Budget(u32),
}

impl ReasoningEffort {
    /// 字符串关键字；`Budget(_)` 返回 `None`（需要序列化成数字）。
    pub fn as_keyword(&self) -> Option<&'static str> {
        match self {
            Self::None => Some("none"),
            Self::Minimal => Some("minimal"),
            Self::Low => Some("low"),
            Self::Medium => Some("medium"),
            Self::High => Some("high"),
            Self::XHigh => Some("xhigh"),
            Self::Max => Some("max"),
            Self::Budget(_) => None,
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::XHigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    /// Anthropic / Gemini 按阈值把关键字映射回一个 token budget；adapter 往上游写
    /// `budget_tokens` / `thinkingBudget` 时用。阈值和 rust-genai + 社区惯例一致。
    pub fn to_budget_tokens(&self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Minimal => Some(256),
            Self::Low => Some(1024),
            Self::Medium => Some(4096),
            Self::High => Some(16_384),
            Self::XHigh => Some(32_768),
            Self::Max => Some(64_000),
            Self::Budget(n) => Some(*n),
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Budget(n) => write!(f, "{n}"),
            other => f.write_str(other.as_keyword().unwrap_or("")),
        }
    }
}

impl Serialize for ReasoningEffort {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Budget(n) => s.serialize_u32(*n),
            other => s.serialize_str(other.as_keyword().unwrap()),
        }
    }
}

impl<'de> Deserialize<'de> for ReasoningEffort {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ReasoningEffort;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a reasoning-effort keyword or token budget number")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                ReasoningEffort::from_keyword(v)
                    .or_else(|| v.parse::<u32>().ok().map(ReasoningEffort::Budget))
                    .ok_or_else(|| E::custom(format!("unknown reasoning_effort: {v}")))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                u32::try_from(v)
                    .map(ReasoningEffort::Budget)
                    .map_err(|_| E::custom("reasoning_effort budget overflow u32"))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                u32::try_from(v.max(0))
                    .map(ReasoningEffort::Budget)
                    .map_err(|_| E::custom("reasoning_effort budget overflow u32"))
            }
        }
        d.deserialize_any(V)
    }
}

// ---------------------------------------------------------------------------
// Verbosity
// ---------------------------------------------------------------------------

/// 回答详尽度提示（GPT-5 系列）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    Low,
    Medium,
    High,
}

impl fmt::Display for Verbosity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        })
    }
}

// ---------------------------------------------------------------------------
// ServiceTier
// ---------------------------------------------------------------------------

/// OpenAI 的服务等级偏好。
///
/// - `Auto`：默认，按账户配额动态选
/// - `Default`：标准处理
/// - `Flex`：弹性处理（延迟高、单价低）
/// - `Priority`：优先级（SLA 账户，单价高）
/// - `Scale`：OpenAI Enterprise 的 scale tier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Priority,
    Scale,
}

impl fmt::Display for ServiceTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Default => "default",
            Self::Flex => "flex",
            Self::Priority => "priority",
            Self::Scale => "scale",
        })
    }
}

// ---------------------------------------------------------------------------
// WebSearchOptions
// ---------------------------------------------------------------------------

/// OpenAI chat.completions 的 `web_search_options`（GPT-4o-search-preview 等）。
///
/// 目前只强类型化 `search_context_size`（常用且枚举值稳定）；`user_location` 的结构
/// 会随 OpenAI 版本变化，保留 `Value` 透传更安全。未知扩展字段 flatten 进 `extra`
/// —— OpenAI 未来加字段 / 第三方兼容网关加私货都不会被吞掉。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSearchOptions {
    /// 搜索上下文体量（越大越贵越精准）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<WebSearchContextSize>,
    /// 近似地理位置（`{type:"approximate", approximate:{city,country,region,timezone}}`）。
    /// 结构上游会变，保持 `Value` 透传。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<serde_json::Value>,
    /// 未知字段兜底（前向兼容）。
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl WebSearchOptions {
    pub fn with_context_size(mut self, size: WebSearchContextSize) -> Self {
        self.search_context_size = Some(size);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSearchContextSize {
    Low,
    Medium,
    High,
}

impl fmt::Display for WebSearchContextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reasoning_effort_keyword_roundtrip() {
        let e = ReasoningEffort::High;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v, json!("high"));
        let back: ReasoningEffort = serde_json::from_value(v).unwrap();
        assert_eq!(back, ReasoningEffort::High);
    }

    #[test]
    fn reasoning_effort_budget_serializes_as_number() {
        // Budget(n) 必须序列化成数字：Anthropic budget_tokens 上游字段是整型，
        // 若输出字符串 "2048" 会被上游 400。
        let e = ReasoningEffort::Budget(2048);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v, json!(2048));
        let back: ReasoningEffort = serde_json::from_value(v).unwrap();
        assert_eq!(back, ReasoningEffort::Budget(2048));
    }

    #[test]
    fn reasoning_effort_accepts_numeric_string_as_budget() {
        // 客户端可能写 "2048"（quote 了）传进来；兼容 fallback 到 Budget。
        let e: ReasoningEffort = serde_json::from_value(json!("2048")).unwrap();
        assert_eq!(e, ReasoningEffort::Budget(2048));
    }

    #[test]
    fn reasoning_effort_rejects_unknown_keyword() {
        let err = serde_json::from_value::<ReasoningEffort>(json!("insane")).unwrap_err();
        assert!(err.to_string().contains("unknown reasoning_effort"));
    }

    #[test]
    fn verbosity_lowercase_roundtrip() {
        let v = serde_json::to_value(Verbosity::Medium).unwrap();
        assert_eq!(v, json!("medium"));
        let back: Verbosity = serde_json::from_value(v).unwrap();
        assert_eq!(back, Verbosity::Medium);
    }

    #[test]
    fn service_tier_lowercase_roundtrip() {
        let v = serde_json::to_value(ServiceTier::Flex).unwrap();
        assert_eq!(v, json!("flex"));
        let back: ServiceTier = serde_json::from_value(v).unwrap();
        assert_eq!(back, ServiceTier::Flex);
    }

    #[test]
    fn service_tier_priority_roundtrip() {
        // Priority 是 SLA 账户用的，不能被 rename_all 把名字改没。
        let v = serde_json::to_value(ServiceTier::Priority).unwrap();
        assert_eq!(v, json!("priority"));
    }

    #[test]
    fn web_search_options_roundtrip_with_context_and_extra() {
        let raw = json!({
            "search_context_size": "high",
            "user_location": {"type": "approximate"},
            "custom_key": "custom_value"
        });
        let opts: WebSearchOptions = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(opts.search_context_size, Some(WebSearchContextSize::High));
        assert_eq!(opts.user_location, Some(json!({"type": "approximate"})));
        assert_eq!(opts.extra.get("custom_key"), Some(&json!("custom_value")));
        // 回写保持所有字段可见。
        let back = serde_json::to_value(&opts).unwrap();
        assert_eq!(back, raw);
    }

    #[test]
    fn web_search_options_empty_skips_fields() {
        // 空 options 序列化时不要输出 `"search_context_size": null` 这种垃圾。
        let v = serde_json::to_value(WebSearchOptions::default()).unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn reasoning_effort_to_budget_tokens_matches_rust_genai_thresholds() {
        // billing 按 budget 计费时用这些阈值映射回 token 数；保持和 rust-genai
        // 的阈值对齐避免 Anthropic thinking 费用估算失真。
        assert_eq!(ReasoningEffort::Low.to_budget_tokens(), Some(1024));
        assert_eq!(ReasoningEffort::Medium.to_budget_tokens(), Some(4096));
        assert_eq!(ReasoningEffort::High.to_budget_tokens(), Some(16_384));
        assert_eq!(ReasoningEffort::Budget(7777).to_budget_tokens(), Some(7777));
        assert_eq!(ReasoningEffort::None.to_budget_tokens(), None);
    }
}
