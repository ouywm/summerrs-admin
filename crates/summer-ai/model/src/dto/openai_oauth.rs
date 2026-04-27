use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOpenAiOAuthAuthUrlDto {
    #[validate(length(min = 1, max = 512, message = "redirectUri 长度必须在1-512之间"))]
    pub redirect_uri: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeOpenAiOAuthCodeDto {
    #[validate(length(min = 1, max = 128, message = "sessionId 长度必须在1-128之间"))]
    pub session_id: String,
    #[validate(length(min = 1, max = 4096, message = "code 长度必须在1-4096之间"))]
    pub code: String,
    #[validate(length(min = 1, max = 128, message = "state 长度必须在1-128之间"))]
    pub state: String,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    #[validate(length(min = 1, max = 128, message = "账号名称长度必须在1-128之间"))]
    pub name: Option<String>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
    #[validate(length(max = 128, message = "测速模型名长度不能超过128"))]
    pub test_model: Option<String>,
}

impl ExchangeOpenAiOAuthCodeDto {
    pub fn validate_target(&self) -> Result<(), String> {
        if self.channel_id.is_some_and(|channel_id| channel_id <= 0) {
            return Err("channelId 必须大于 0".to_string());
        }
        if self.account_id.is_some_and(|account_id| account_id <= 0) {
            return Err("accountId 必须大于 0".to_string());
        }
        if self
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err("name 不能为空白".to_string());
        }

        let has_name = self
            .name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        let is_create = self.channel_id.is_some() && self.account_id.is_none() && has_name;
        let is_update = self.channel_id.is_none() && self.account_id.is_some();

        if is_create || is_update {
            return Ok(());
        }

        Err("exchange 目标必须满足 create(channelId + name) 或 update(accountId)".to_string())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct RefreshOpenAiOAuthTokenDto {
    #[validate(range(min = 1, message = "accountId 必须大于 0"))]
    pub account_id: i64,
}
