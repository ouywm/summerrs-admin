use serde::Deserialize;

/// 创建用户请求
#[derive(Debug, Deserialize)]
pub struct CreateUserDto {
    pub username: String,
    pub password: String,
    pub nickname: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

/// 更新用户请求
#[derive(Debug, Deserialize)]
pub struct UpdateUserDto {
    pub nickname: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
    pub status: Option<i16>,
}

/// 重置密码请求
#[derive(Debug, Deserialize)]
pub struct ResetPasswordDto {
    pub new_password: String,
}
