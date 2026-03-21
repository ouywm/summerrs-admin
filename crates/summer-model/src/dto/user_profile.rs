use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_user::{self, Gender};

/// 修改个人密码请求参数
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordDto {
    /// 当前密码
    #[validate(length(min = 1, message = "请输入当前密码"))]
    pub old_password: String,

    /// 新密码（长度至少6位）
    #[validate(length(min = 6, message = "新密码长度至少6位"))]
    pub new_password: String,
}

/// 更新个人信息请求参数
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileDto {
    /// 昵称
    #[validate(length(min = 1, max = 64, message = "昵称长度必须在1-64之间"))]
    pub nick_name: Option<String>,

    /// 邮箱
    #[validate(email(message = "邮箱格式不正确"))]
    pub email: Option<String>,

    /// 手机号
    #[validate(length(max = 32, message = "手机号长度不能超过32"))]
    pub phone: Option<String>,

    /// 性别
    pub gender: Option<Gender>,

    /// 头像URL
    #[validate(length(max = 512, message = "头像URL长度不能超过512"))]
    pub avatar: Option<String>,
}

impl UpdateProfileDto {
    /// 将 DTO 中的非空字段应用到 ActiveModel
    pub fn apply_to(self, active: &mut sys_user::ActiveModel) {
        if let Some(nick_name) = self.nick_name {
            active.nick_name = Set(nick_name);
        }
        if let Some(email) = self.email {
            active.email = Set(email);
        }
        if let Some(phone) = self.phone {
            active.phone = Set(phone);
        }
        if let Some(gender) = self.gender {
            active.gender = Set(gender);
        }
        if let Some(avatar) = self.avatar {
            active.avatar = Set(avatar);
        }
    }
}
