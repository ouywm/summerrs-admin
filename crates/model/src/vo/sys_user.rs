use chrono::NaiveDateTime;
use common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use super::sys_role::RoleDetailVo;
use crate::entity::sys_user::{self, Gender, UserStatus};

/// 登录后获取的用户信息（含角色和按钮权限）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserInfoVo {
    pub user_id: i64,
    pub user_name: String,
    pub email: String,
    pub avatar: String,
    pub roles: Vec<String>,
    pub buttons: Vec<String>,
}

/// 用户列表项（字段名匹配前端）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserVo {
    pub id: i64,
    pub avatar: String,
    pub status: UserStatus,
    pub user_name: String,
    pub user_gender: String,
    pub nick_name: String,
    pub user_phone: String,
    pub user_email: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl UserVo {
    /// 从 Entity Model 构建 UserVo
    pub fn from_model(model: sys_user::Model) -> Self {
        Self {
            id: model.id,
            avatar: model.avatar,
            status: model.status,
            user_name: model.user_name,
            user_gender: match model.gender {
                Gender::Unknown => "未知",
                Gender::Male => "男",
                Gender::Female => "女",
            }
            .to_string(),
            nick_name: model.nick_name,
            user_phone: model.phone,
            user_email: model.email,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

/// 用户详情（含角色详细信息）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDetailVo {
    #[serde(flatten)]
    pub user: UserVo,
    pub roles: Vec<RoleDetailVo>,
}
