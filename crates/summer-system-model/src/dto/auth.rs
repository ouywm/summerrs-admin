use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, JsonSchema, Serialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct LoginDto {
    #[validate(length(min = 1, max = 64, message = "用户名不能为空"))]
    pub user_name: String,
    #[validate(length(min = 1, max = 128, message = "密码不能为空"))]
    pub password: String,
}

/// B 端登录
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct BizLoginDto {
    #[validate(length(min = 1, message = "用户名不能为空"))]
    pub user_name: String,
    #[validate(length(min = 1, message = "密码不能为空"))]
    pub password: String,
}

/// C 端登录（手机号 + 密码）
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CustomerLoginDto {
    #[validate(length(min = 1, message = "手机号不能为空"))]
    pub phone: String,
    #[validate(length(min = 1, message = "密码不能为空"))]
    pub password: String,
}

/// Token 刷新请求
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenDto {
    #[validate(length(min = 1, message = "refresh_token 不能为空"))]
    pub refresh_token: String,
}
