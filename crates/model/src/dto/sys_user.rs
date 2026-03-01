use schemars::JsonSchema;
use sea_orm::Set;
use serde::Deserialize;
use validator::Validate;
use crate::entity::sys_user::{self, Gender, UserStatus};

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserDto {
    #[validate(length(min = 1, max = 64, message = "用户名长度必须在1-64之间"))]
    pub user_name: String,
    #[validate(length(min = 1, max = 64, message = "昵称长度必须在1-64之间"))]
    pub nick_name: String,
    pub gender: Option<Gender>,
    #[validate(length(max = 32, message = "手机号长度不能超过32"))]
    pub phone: Option<String>,
    #[validate(email(message = "邮箱格式不正确"))]
    pub email: Option<String>,
    #[validate(length(max = 512, message = "头像URL长度不能超过512"))]
    pub avatar: Option<String>,
    pub status: Option<UserStatus>,
    pub role_ids: Option<Vec<i64>>,
}

impl CreateUserDto {
    /// 转换为 ActiveModel
    pub fn into_active_model(self, hashed_password: String, operator: String) -> sys_user::ActiveModel {
        sys_user::ActiveModel {
            user_name: Set(self.user_name),
            password: Set(hashed_password),
            nick_name: Set(self.nick_name),
            gender: Set(self.gender.unwrap_or(Gender::Unknown)),
            phone: Set(self.phone.unwrap_or_default()),
            email: Set(self.email.unwrap_or_default()),
            avatar: Set(self.avatar.unwrap_or_default()),
            status: Set(self.status.unwrap_or(UserStatus::Online)),
            create_by: Set(operator.clone()),
            update_by: Set(operator),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserDto {
    #[validate(length(min = 1, max = 64, message = "昵称长度必须在1-64之间"))]
    pub nick_name: Option<String>,
    pub gender: Option<Gender>,
    #[validate(length(max = 32, message = "手机号长度不能超过32"))]
    pub phone: Option<String>,
    #[validate(email(message = "邮箱格式不正确"))]
    pub email: Option<String>,
    #[validate(length(max = 512, message = "头像URL长度不能超过512"))]
    pub avatar: Option<String>,
    pub status: Option<UserStatus>,
    pub role_ids: Option<Vec<i64>>,
}

impl UpdateUserDto {
    /// 将 DTO 中的非空字段应用到 ActiveModel
    pub fn apply_to(self, active: &mut sys_user::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(nick_name) = self.nick_name {
            active.nick_name = Set(nick_name);
        }
        if let Some(gender) = self.gender {
            active.gender = Set(gender);
        }
        if let Some(phone) = self.phone {
            active.phone = Set(phone);
        }
        if let Some(email) = self.email {
            active.email = Set(email);
        }
        if let Some(avatar) = self.avatar {
            active.avatar = Set(avatar);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserQueryDto {
    pub user_name: Option<String>,
    pub user_phone: Option<String>,
    pub user_email: Option<String>,
    pub status: Option<UserStatus>,
    pub user_gender: Option<Gender>,
}

/// 重置密码请求参数
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordDto {
    /// 新密码（长度至少6位）
    #[validate(length(min = 6, message = "密码长度至少6位"))]
    pub new_password: String,
}
