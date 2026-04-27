use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiOAuthAuthUrlVo {
    pub auth_url: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiOAuthExchangeVo {
    pub account_id: i64,
    pub created: bool,
    pub expires_at: DateTimeWithTimeZone,
    pub subscription_expires_at: Option<DateTimeWithTimeZone>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiOAuthRefreshVo {
    pub account_id: i64,
    pub refreshed_at: DateTimeWithTimeZone,
    pub expires_at: DateTimeWithTimeZone,
    pub subscription_expires_at: Option<DateTimeWithTimeZone>,
}
