use chrono::NaiveDateTime;
use serde::Serialize;

/// 用户信息响应
#[derive(Debug, Serialize)]
pub struct UserVo {
    pub id: i64,
    pub username: String,
    pub nickname: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
    pub status: i16,
    pub created_at: NaiveDateTime,
}

impl From<crate::entity::sys_user::Model> for UserVo {
    fn from(m: crate::entity::sys_user::Model) -> Self {
        Self {
            id: m.id,
            username: m.username,
            nickname: m.nickname,
            email: m.email,
            phone: m.phone,
            avatar: m.avatar,
            status: m.status,
            created_at: m.created_at,
        }
    }
}
