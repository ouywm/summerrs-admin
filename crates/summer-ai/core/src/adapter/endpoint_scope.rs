//! [`EndpointScope`] —— 路由用的"端点家族"枚举。
//!
//! 作用：决定同一个 channel 在不同 HTTP 入口上走哪个 adapter。
//!
//! - `/v1/chat/completions`、`/v1/messages`、`/v1beta/…:generateContent` → `Chat`
//! - `/v1/responses` → `Responses`
//! - `/v1/embeddings` → `Embeddings`
//! - `/v1/images/generations` → `Images`
//! - `/v1/audio/*` → `Audio`
//!
//! 绑定 `ai.channel.endpoint_scopes`（JSONB `Vec<String>`）+ `ai.token.endpoint_scopes`。
//! DB 里是字符串，代码里是强枚举；未知字符串解析时 drop + warn，不让 admin 配错
//! 把路由带歪。

use serde::{Deserialize, Serialize};

/// 端点家族。
///
/// 编码值按出现顺序；当前仅 `Chat` 和 `Responses` 参与路由，其他变体预留给
/// embeddings / images / audio 端点后续接入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointScope {
    /// `/v1/chat/completions`、`/v1/messages`、`/v1beta/models/{m}:generateContent`
    Chat,
    /// `/v1/responses`（OpenAI Responses API，GPT-5 / o1 reasoning）
    Responses,
    /// `/v1/embeddings`
    Embeddings,
    /// `/v1/images/generations`
    Images,
    /// `/v1/audio/speech` + `/v1/audio/transcriptions`
    Audio,
}

impl EndpointScope {
    /// 所有变体按出现顺序。
    pub const ALL: [EndpointScope; 5] = [
        Self::Chat,
        Self::Responses,
        Self::Embeddings,
        Self::Images,
        Self::Audio,
    ];

    /// JSON / DB 存储用的小写字符串。
    pub const fn as_lower_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
            Self::Embeddings => "embeddings",
            Self::Images => "images",
            Self::Audio => "audio",
        }
    }

    /// 从小写字符串解析。
    pub fn from_lower_str(s: &str) -> Option<Self> {
        Some(match s {
            "chat" => Self::Chat,
            "responses" => Self::Responses,
            "embeddings" => Self::Embeddings,
            "images" => Self::Images,
            "audio" => Self::Audio,
            _ => return None,
        })
    }
}

impl std::fmt::Display for EndpointScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_lower_str())
    }
}

impl std::str::FromStr for EndpointScope {
    type Err = UnknownEndpointScope;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_lower_str(s).ok_or_else(|| UnknownEndpointScope(s.to_string()))
    }
}

/// 未知 endpoint scope 字符串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEndpointScope(pub String);

impl std::fmt::Display for UnknownEndpointScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown endpoint scope: {}", self.0)
    }
}

impl std::error::Error for UnknownEndpointScope {}

/// 从 JSON `Vec<String>`(来自 `ai.channel.endpoint_scopes` / `ai.token.endpoint_scopes`)
/// 解析 `Vec<EndpointScope>`。
///
/// - 非数组 → 返空 `Vec`
/// - 数组里的非字符串 / 无法识别的 scope → drop + `warn!`
///
/// 这样 admin 误配("responce" 漏 s) 不会导致反序列化失败,只会让该 scope 失效
/// (channel 不会被选到 Responses 端点),并在日志里留下证据。
pub fn parse_json_scopes(value: &serde_json::Value) -> Vec<EndpointScope> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            let s = v.as_str()?;
            match EndpointScope::from_lower_str(s) {
                Some(scope) => Some(scope),
                None => {
                    tracing::warn!(raw = %s, "unknown endpoint_scope value, dropped");
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_roundtrip_via_lower_str() {
        for scope in EndpointScope::ALL {
            let s = scope.as_lower_str();
            let back = EndpointScope::from_lower_str(s).unwrap();
            assert_eq!(
                scope, back,
                "lower_str roundtrip failed for {scope:?} ({s})"
            );
        }
    }

    #[test]
    fn serde_snake_case_roundtrip() {
        let scope = EndpointScope::Responses;
        let s = serde_json::to_string(&scope).unwrap();
        assert_eq!(s, r#""responses""#);
        let back: EndpointScope = serde_json::from_str(&s).unwrap();
        assert_eq!(scope, back);
    }

    #[test]
    fn from_str_rejects_unknown() {
        assert!("whatever".parse::<EndpointScope>().is_err());
        assert_eq!(
            "responses".parse::<EndpointScope>().unwrap(),
            EndpointScope::Responses
        );
    }

    #[test]
    fn parse_json_scopes_drops_unknown_and_keeps_known() {
        let v = serde_json::json!(["chat", "responses", "responce", "embeddings", 42, null]);
        let scopes = parse_json_scopes(&v);
        assert_eq!(
            scopes,
            vec![
                EndpointScope::Chat,
                EndpointScope::Responses,
                EndpointScope::Embeddings
            ]
        );
    }

    #[test]
    fn parse_json_scopes_non_array_returns_empty() {
        assert!(parse_json_scopes(&serde_json::json!("chat")).is_empty());
        assert!(parse_json_scopes(&serde_json::json!({"x": 1})).is_empty());
        assert!(parse_json_scopes(&serde_json::Value::Null).is_empty());
    }

    #[test]
    fn display_matches_lower_str() {
        assert_eq!(format!("{}", EndpointScope::Chat), "chat");
        assert_eq!(format!("{}", EndpointScope::Responses), "responses");
    }
}
