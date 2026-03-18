use common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::{sys_notice, sys_notice_user, sys_role, sys_user};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeVo {
    pub id: i64,
    pub notice_title: String,
    pub notice_level: sys_notice::NoticeLevel,
    pub notice_scope: sys_notice::NoticeScope,
    pub publish_status: sys_notice::PublishStatus,
    pub pinned: bool,
    pub enabled: bool,
    pub sort: i32,
    pub publish_by: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub publish_time: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expire_time: Option<chrono::NaiveDateTime>,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: chrono::NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: chrono::NaiveDateTime,
}

impl From<sys_notice::Model> for NoticeVo {
    fn from(model: sys_notice::Model) -> Self {
        Self {
            id: model.id,
            notice_title: model.notice_title,
            notice_level: model.notice_level,
            notice_scope: model.notice_scope,
            publish_status: model.publish_status,
            pinned: model.pinned,
            enabled: model.enabled,
            sort: model.sort,
            publish_by: model.publish_by,
            publish_time: model.publish_time,
            expire_time: model.expire_time,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeTargetRoleVo {
    pub role_id: i64,
    pub role_name: String,
    pub role_code: String,
}

impl From<sys_role::Model> for NoticeTargetRoleVo {
    fn from(model: sys_role::Model) -> Self {
        Self {
            role_id: model.id,
            role_name: model.role_name,
            role_code: model.role_code,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeTargetUserVo {
    pub user_id: i64,
    pub user_name: String,
    pub nick_name: String,
}

impl From<sys_user::Model> for NoticeTargetUserVo {
    fn from(model: sys_user::Model) -> Self {
        Self {
            user_id: model.id,
            user_name: model.user_name,
            nick_name: model.nick_name,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeDetailVo {
    pub id: i64,
    pub notice_title: String,
    pub notice_content: String,
    pub notice_level: sys_notice::NoticeLevel,
    pub notice_scope: sys_notice::NoticeScope,
    pub publish_status: sys_notice::PublishStatus,
    pub target_role_ids: Vec<i64>,
    pub target_roles: Vec<NoticeTargetRoleVo>,
    pub target_user_ids: Vec<i64>,
    pub target_users: Vec<NoticeTargetUserVo>,
    pub pinned: bool,
    pub enabled: bool,
    pub sort: i32,
    pub publish_by: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub publish_time: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expire_time: Option<chrono::NaiveDateTime>,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: chrono::NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: chrono::NaiveDateTime,
}

impl NoticeDetailVo {
    pub fn from_model(
        model: sys_notice::Model,
        target_role_ids: Vec<i64>,
        target_roles: Vec<NoticeTargetRoleVo>,
        target_user_ids: Vec<i64>,
        target_users: Vec<NoticeTargetUserVo>,
    ) -> Self {
        Self {
            id: model.id,
            notice_title: model.notice_title,
            notice_content: model.notice_content,
            notice_level: model.notice_level,
            notice_scope: model.notice_scope,
            publish_status: model.publish_status,
            target_role_ids,
            target_roles,
            target_user_ids,
            target_users,
            pinned: model.pinned,
            enabled: model.enabled,
            sort: model.sort,
            publish_by: model.publish_by,
            publish_time: model.publish_time,
            expire_time: model.expire_time,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserNoticeVo {
    pub id: i64,
    pub notice_title: String,
    pub notice_level: sys_notice::NoticeLevel,
    pub pinned: bool,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub publish_time: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expire_time: Option<chrono::NaiveDateTime>,
    pub read_flag: bool,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub read_time: Option<chrono::NaiveDateTime>,
}

impl UserNoticeVo {
    pub fn from_models(notice: sys_notice::Model, notice_user: sys_notice_user::Model) -> Self {
        Self {
            id: notice.id,
            notice_title: notice.notice_title,
            notice_level: notice.notice_level,
            pinned: notice.pinned,
            publish_time: notice.publish_time,
            expire_time: notice.expire_time,
            read_flag: notice_user.read_flag,
            read_time: notice_user.read_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserNoticeDetailVo {
    pub id: i64,
    pub notice_title: String,
    pub notice_content: String,
    pub notice_level: sys_notice::NoticeLevel,
    pub pinned: bool,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub publish_time: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expire_time: Option<chrono::NaiveDateTime>,
    pub read_flag: bool,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub read_time: Option<chrono::NaiveDateTime>,
}

impl UserNoticeDetailVo {
    pub fn from_models(notice: sys_notice::Model, notice_user: sys_notice_user::Model) -> Self {
        Self {
            id: notice.id,
            notice_title: notice.notice_title,
            notice_content: notice.notice_content,
            notice_level: notice.notice_level,
            pinned: notice.pinned,
            publish_time: notice.publish_time,
            expire_time: notice.expire_time,
            read_flag: notice_user.read_flag,
            read_time: notice_user.read_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeUnreadCountVo {
    pub unread_count: u64,
}
