use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::{sys_notice, sys_notice_user};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoticeDto {
    #[validate(length(min = 1, max = 200, message = "公告标题长度必须在1-200之间"))]
    pub notice_title: String,
    #[validate(length(min = 1, message = "公告正文不能为空"))]
    pub notice_content: String,
    pub notice_level: Option<sys_notice::NoticeLevel>,
    pub notice_scope: Option<sys_notice::NoticeScope>,
    pub target_role_ids: Option<Vec<i64>>,
    pub target_user_ids: Option<Vec<i64>>,
    pub pinned: Option<bool>,
    pub enabled: Option<bool>,
    pub sort: Option<i32>,
    pub expire_time: Option<chrono::NaiveDateTime>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateNoticeDto {
    pub fn into_active_model(self, operator: String) -> sys_notice::ActiveModel {
        sys_notice::ActiveModel {
            id: NotSet,
            notice_title: Set(self.notice_title),
            notice_content: Set(self.notice_content),
            notice_level: Set(self.notice_level.unwrap_or(sys_notice::NoticeLevel::Normal)),
            notice_scope: Set(self
                .notice_scope
                .unwrap_or(sys_notice::NoticeScope::AllAdmin)),
            publish_status: Set(sys_notice::PublishStatus::Draft),
            pinned: Set(self.pinned.unwrap_or(false)),
            enabled: Set(self.enabled.unwrap_or(true)),
            sort: Set(self.sort.unwrap_or(0)),
            publish_by: Set(String::new()),
            publish_time: Set(None),
            expire_time: Set(self.expire_time),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            create_time: NotSet,
            update_by: Set(operator),
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoticeDto {
    #[validate(length(min = 1, max = 200, message = "公告标题长度必须在1-200之间"))]
    pub notice_title: Option<String>,
    #[validate(length(min = 1, message = "公告正文不能为空"))]
    pub notice_content: Option<String>,
    pub notice_level: Option<sys_notice::NoticeLevel>,
    pub notice_scope: Option<sys_notice::NoticeScope>,
    pub target_role_ids: Option<Vec<i64>>,
    pub target_user_ids: Option<Vec<i64>>,
    pub pinned: Option<bool>,
    pub enabled: Option<bool>,
    pub sort: Option<i32>,
    pub expire_time: Option<Option<chrono::NaiveDateTime>>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateNoticeDto {
    pub fn apply_to(self, active: &mut sys_notice::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(value) = self.notice_title {
            active.notice_title = Set(value);
        }
        if let Some(value) = self.notice_content {
            active.notice_content = Set(value);
        }
        if let Some(value) = self.notice_level {
            active.notice_level = Set(value);
        }
        if let Some(value) = self.notice_scope {
            active.notice_scope = Set(value);
        }
        if let Some(value) = self.pinned {
            active.pinned = Set(value);
        }
        if let Some(value) = self.enabled {
            active.enabled = Set(value);
        }
        if let Some(value) = self.sort {
            active.sort = Set(value);
        }
        if let Some(value) = self.expire_time {
            active.expire_time = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeQueryDto {
    pub id: Option<i64>,
    pub notice_title: Option<String>,
    pub notice_level: Option<sys_notice::NoticeLevel>,
    pub notice_scope: Option<sys_notice::NoticeScope>,
    pub publish_status: Option<sys_notice::PublishStatus>,
    pub pinned: Option<bool>,
    pub enabled: Option<bool>,
    pub publish_time_start: Option<chrono::NaiveDateTime>,
    pub publish_time_end: Option<chrono::NaiveDateTime>,
    pub create_time_start: Option<chrono::NaiveDateTime>,
    pub create_time_end: Option<chrono::NaiveDateTime>,
}

impl From<NoticeQueryDto> for Condition {
    fn from(query: NoticeQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(value) = query.id {
            cond = cond.add(sys_notice::Column::Id.eq(value));
        }
        if let Some(value) = query.notice_title {
            cond = cond.add(sys_notice::Column::NoticeTitle.contains(value));
        }
        if let Some(value) = query.notice_level {
            cond = cond.add(sys_notice::Column::NoticeLevel.eq(value));
        }
        if let Some(value) = query.notice_scope {
            cond = cond.add(sys_notice::Column::NoticeScope.eq(value));
        }
        if let Some(value) = query.publish_status {
            cond = cond.add(sys_notice::Column::PublishStatus.eq(value));
        }
        if let Some(value) = query.pinned {
            cond = cond.add(sys_notice::Column::Pinned.eq(value));
        }
        if let Some(value) = query.enabled {
            cond = cond.add(sys_notice::Column::Enabled.eq(value));
        }
        if let Some(start) = query.publish_time_start {
            cond = cond.add(sys_notice::Column::PublishTime.gte(start));
        }
        if let Some(end) = query.publish_time_end {
            cond = cond.add(sys_notice::Column::PublishTime.lte(end));
        }
        if let Some(start) = query.create_time_start {
            cond = cond.add(sys_notice::Column::CreateTime.gte(start));
        }
        if let Some(end) = query.create_time_end {
            cond = cond.add(sys_notice::Column::CreateTime.lte(end));
        }
        cond
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserNoticeQueryDto {
    pub notice_title: Option<String>,
    pub notice_level: Option<sys_notice::NoticeLevel>,
    pub read_flag: Option<bool>,
}

impl From<UserNoticeQueryDto> for Condition {
    fn from(query: UserNoticeQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(value) = query.notice_title {
            cond = cond.add(sys_notice::Column::NoticeTitle.contains(value));
        }
        if let Some(value) = query.notice_level {
            cond = cond.add(sys_notice::Column::NoticeLevel.eq(value));
        }
        if let Some(value) = query.read_flag {
            cond = cond.add(sys_notice_user::Column::ReadFlag.eq(value));
        }
        cond
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserNoticeLatestQueryDto {
    pub size: Option<u64>,
    pub read_flag: Option<bool>,
}

impl UserNoticeLatestQueryDto {
    pub fn limit(&self) -> u64 {
        self.size.unwrap_or(5).clamp(1, 20)
    }
}
