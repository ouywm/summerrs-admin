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
