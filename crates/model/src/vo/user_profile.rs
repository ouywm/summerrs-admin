use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::sys_user::{Gender, Model};

/// 用户个人信息响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileVo {
    pub user_id: i64,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phone: String,
    pub gender: Gender,
    pub avatar: String,
    pub update_time: String,
}

impl UserProfileVo {
    pub fn from_model(model: Model) -> Self {
        Self {
            user_id: model.id,
            user_name: model.user_name,
            nick_name: model.nick_name,
            email: model.email,
            phone: model.phone,
            gender: model.gender,
            avatar: model.avatar,
            update_time: model.update_time.format("%Y-%m-%dT%H:%M:%S").to_string(),
        }
    }
}
