//! Generated admin DTO skeleton.

use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use sea_orm::prelude::Decimal;

use crate::entity::biz_showcase_profile;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateShowcaseProfileDto {
    /// 展示编码
    pub showcase_code: String,

    /// 标题
    pub title: String,

    /// 头像
    pub avatar: Option<String>,

    /// 封面图片
    pub cover_image: Option<String>,

    /// 联系人
    pub contact_name: Option<String>,

    /// 联系人性别
    pub contact_gender: Option<i16>,

    /// 联系电话
    pub contact_phone: Option<String>,

    /// 联系邮箱
    pub contact_email: Option<String>,

    /// 官网链接
    pub official_url: Option<String>,

    /// 状态
    pub status: Option<i16>,

    /// 推荐
    pub featured: Option<bool>,

    /// 优先级
    pub priority: Option<i32>,

    #[schemars(with = "Option<String>")]

    /// 评分
    pub score: Option<Decimal>,

    /// 发布日期
    pub publish_date: Option<chrono::NaiveDate>,

    /// 上线时间
    pub launch_at: Option<chrono::NaiveDateTime>,

    /// 服务时间
    pub service_time: Option<chrono::NaiveTime>,

    /// 附件
    pub attachment_url: Option<String>,

    /// 描述
    pub description: Option<String>,

    /// 备注
    pub extra_notes: Option<String>,

    /// 元数据
    pub metadata: Option<serde_json::Value>,

    /// 创建时间
    pub created_at: Option<chrono::NaiveDateTime>,

    /// 更新时间
    pub updated_at: Option<chrono::NaiveDateTime>,
}

impl From<CreateShowcaseProfileDto> for biz_showcase_profile::ActiveModel {
    fn from(dto: CreateShowcaseProfileDto) -> Self {
        Self {
            showcase_code: Set(dto.showcase_code),

            title: Set(dto.title),

            avatar: dto.avatar.map(|value| Set(Some(value))).unwrap_or_default(),

            cover_image: dto
                .cover_image
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            contact_name: dto
                .contact_name
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            contact_gender: dto.contact_gender.map(Set).unwrap_or_default(),

            contact_phone: dto
                .contact_phone
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            contact_email: dto
                .contact_email
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            official_url: dto
                .official_url
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            status: dto.status.map(Set).unwrap_or_default(),

            featured: dto.featured.map(Set).unwrap_or_default(),

            priority: dto.priority.map(Set).unwrap_or_default(),

            score: dto.score.map(|value| Set(Some(value))).unwrap_or_default(),

            publish_date: dto
                .publish_date
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            launch_at: dto
                .launch_at
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            service_time: dto
                .service_time
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            attachment_url: dto
                .attachment_url
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            description: dto
                .description
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            extra_notes: dto
                .extra_notes
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            metadata: dto
                .metadata
                .map(|value| Set(Some(value)))
                .unwrap_or_default(),

            created_at: dto.created_at.map(Set).unwrap_or_default(),

            updated_at: dto.updated_at.map(Set).unwrap_or_default(),

            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShowcaseProfileDto {
    /// 展示编码
    pub showcase_code: Option<String>,

    /// 标题
    pub title: Option<String>,

    /// 头像
    pub avatar: Option<String>,

    /// 封面图片
    pub cover_image: Option<String>,

    /// 联系人
    pub contact_name: Option<String>,

    /// 联系人性别
    pub contact_gender: Option<i16>,

    /// 联系电话
    pub contact_phone: Option<String>,

    /// 联系邮箱
    pub contact_email: Option<String>,

    /// 官网链接
    pub official_url: Option<String>,

    /// 状态
    pub status: Option<i16>,

    /// 推荐
    pub featured: Option<bool>,

    /// 优先级
    pub priority: Option<i32>,

    #[schemars(with = "Option<String>")]

    /// 评分
    pub score: Option<Decimal>,

    /// 发布日期
    pub publish_date: Option<chrono::NaiveDate>,

    /// 上线时间
    pub launch_at: Option<chrono::NaiveDateTime>,

    /// 服务时间
    pub service_time: Option<chrono::NaiveTime>,

    /// 附件
    pub attachment_url: Option<String>,

    /// 描述
    pub description: Option<String>,

    /// 备注
    pub extra_notes: Option<String>,

    /// 元数据
    pub metadata: Option<serde_json::Value>,

    /// 创建时间
    pub created_at: Option<chrono::NaiveDateTime>,

    /// 更新时间
    pub updated_at: Option<chrono::NaiveDateTime>,
}

impl UpdateShowcaseProfileDto {
    pub fn apply_to(self, active: &mut biz_showcase_profile::ActiveModel) {
        if let Some(value) = self.showcase_code {
            active.showcase_code = Set(value);
        }

        if let Some(value) = self.title {
            active.title = Set(value);
        }

        if let Some(value) = self.avatar {
            active.avatar = Set(Some(value));
        }

        if let Some(value) = self.cover_image {
            active.cover_image = Set(Some(value));
        }

        if let Some(value) = self.contact_name {
            active.contact_name = Set(Some(value));
        }

        if let Some(value) = self.contact_gender {
            active.contact_gender = Set(value);
        }

        if let Some(value) = self.contact_phone {
            active.contact_phone = Set(Some(value));
        }

        if let Some(value) = self.contact_email {
            active.contact_email = Set(Some(value));
        }

        if let Some(value) = self.official_url {
            active.official_url = Set(Some(value));
        }

        if let Some(value) = self.status {
            active.status = Set(value);
        }

        if let Some(value) = self.featured {
            active.featured = Set(value);
        }

        if let Some(value) = self.priority {
            active.priority = Set(value);
        }

        if let Some(value) = self.score {
            active.score = Set(Some(value));
        }

        if let Some(value) = self.publish_date {
            active.publish_date = Set(Some(value));
        }

        if let Some(value) = self.launch_at {
            active.launch_at = Set(Some(value));
        }

        if let Some(value) = self.service_time {
            active.service_time = Set(Some(value));
        }

        if let Some(value) = self.attachment_url {
            active.attachment_url = Set(Some(value));
        }

        if let Some(value) = self.description {
            active.description = Set(Some(value));
        }

        if let Some(value) = self.extra_notes {
            active.extra_notes = Set(Some(value));
        }

        if let Some(value) = self.metadata {
            active.metadata = Set(Some(value));
        }

        if let Some(value) = self.created_at {
            active.created_at = Set(value);
        }

        if let Some(value) = self.updated_at {
            active.updated_at = Set(value);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowcaseProfileQueryDto {
    /// 主键
    pub id: Option<i64>,

    /// 展示编码
    pub showcase_code: Option<String>,

    /// 标题
    pub title: Option<String>,

    /// 头像
    pub avatar: Option<String>,

    /// 封面图片
    pub cover_image: Option<String>,

    /// 联系人
    pub contact_name: Option<String>,

    /// 联系人性别
    pub contact_gender: Option<i16>,

    /// 联系电话
    pub contact_phone: Option<String>,

    /// 联系邮箱
    pub contact_email: Option<String>,

    /// 官网链接
    pub official_url: Option<String>,

    /// 状态
    pub status: Option<i16>,

    /// 推荐
    pub featured: Option<bool>,

    /// 优先级
    pub priority: Option<i32>,

    #[schemars(with = "Option<String>")]

    /// 评分
    pub score: Option<Decimal>,

    /// 发布日期
    pub publish_date: Option<chrono::NaiveDate>,

    /// 发布日期开始
    pub publish_date_start: Option<chrono::NaiveDate>,

    /// 发布日期结束
    pub publish_date_end: Option<chrono::NaiveDate>,

    /// 上线时间
    pub launch_at: Option<chrono::NaiveDateTime>,

    /// 上线时间开始
    pub launch_at_start: Option<chrono::NaiveDateTime>,

    /// 上线时间结束
    pub launch_at_end: Option<chrono::NaiveDateTime>,

    /// 服务时间
    pub service_time: Option<chrono::NaiveTime>,

    /// 附件
    pub attachment_url: Option<String>,

    /// 描述
    pub description: Option<String>,

    /// 备注
    pub extra_notes: Option<String>,

    /// 元数据
    pub metadata: Option<serde_json::Value>,

    /// 创建时间
    pub created_at: Option<chrono::NaiveDateTime>,

    /// 创建时间开始
    pub created_at_start: Option<chrono::NaiveDateTime>,

    /// 创建时间结束
    pub created_at_end: Option<chrono::NaiveDateTime>,

    /// 更新时间
    pub updated_at: Option<chrono::NaiveDateTime>,

    /// 更新时间开始
    pub updated_at_start: Option<chrono::NaiveDateTime>,

    /// 更新时间结束
    pub updated_at_end: Option<chrono::NaiveDateTime>,
}

impl From<ShowcaseProfileQueryDto> for Condition {
    fn from(query: ShowcaseProfileQueryDto) -> Self {
        let mut cond = Condition::all();

        if let Some(value) = query.id {
            cond = cond.add(biz_showcase_profile::Column::Id.eq(value));
        }

        if let Some(value) = query.showcase_code {
            cond = cond.add(biz_showcase_profile::Column::ShowcaseCode.contains(value));
        }

        if let Some(value) = query.title {
            cond = cond.add(biz_showcase_profile::Column::Title.contains(value));
        }

        if let Some(value) = query.avatar {
            cond = cond.add(biz_showcase_profile::Column::Avatar.contains(value));
        }

        if let Some(value) = query.cover_image {
            cond = cond.add(biz_showcase_profile::Column::CoverImage.contains(value));
        }

        if let Some(value) = query.contact_name {
            cond = cond.add(biz_showcase_profile::Column::ContactName.contains(value));
        }

        if let Some(value) = query.contact_gender {
            cond = cond.add(biz_showcase_profile::Column::ContactGender.eq(value));
        }

        if let Some(value) = query.contact_phone {
            cond = cond.add(biz_showcase_profile::Column::ContactPhone.contains(value));
        }

        if let Some(value) = query.contact_email {
            cond = cond.add(biz_showcase_profile::Column::ContactEmail.contains(value));
        }

        if let Some(value) = query.official_url {
            cond = cond.add(biz_showcase_profile::Column::OfficialUrl.contains(value));
        }

        if let Some(value) = query.status {
            cond = cond.add(biz_showcase_profile::Column::Status.eq(value));
        }

        if let Some(value) = query.featured {
            cond = cond.add(biz_showcase_profile::Column::Featured.eq(value));
        }

        if let Some(value) = query.priority {
            cond = cond.add(biz_showcase_profile::Column::Priority.eq(value));
        }

        if let Some(value) = query.score {
            cond = cond.add(biz_showcase_profile::Column::Score.eq(value));
        }

        if let Some(value) = query.publish_date {
            cond = cond.add(biz_showcase_profile::Column::PublishDate.eq(value));
        }

        if let Some(start) = query.publish_date_start {
            cond = cond.add(biz_showcase_profile::Column::PublishDate.gte(start));
        }
        if let Some(end) = query.publish_date_end {
            cond = cond.add(biz_showcase_profile::Column::PublishDate.lte(end));
        }

        if let Some(value) = query.launch_at {
            cond = cond.add(biz_showcase_profile::Column::LaunchAt.eq(value));
        }

        if let Some(start) = query.launch_at_start {
            cond = cond.add(biz_showcase_profile::Column::LaunchAt.gte(start));
        }
        if let Some(end) = query.launch_at_end {
            cond = cond.add(biz_showcase_profile::Column::LaunchAt.lte(end));
        }

        if let Some(value) = query.service_time {
            cond = cond.add(biz_showcase_profile::Column::ServiceTime.eq(value));
        }

        if let Some(value) = query.attachment_url {
            cond = cond.add(biz_showcase_profile::Column::AttachmentUrl.contains(value));
        }

        if let Some(value) = query.description {
            cond = cond.add(biz_showcase_profile::Column::Description.contains(value));
        }

        if let Some(value) = query.extra_notes {
            cond = cond.add(biz_showcase_profile::Column::ExtraNotes.contains(value));
        }

        if let Some(value) = query.metadata {
            cond = cond.add(biz_showcase_profile::Column::Metadata.eq(value));
        }

        if let Some(value) = query.created_at {
            cond = cond.add(biz_showcase_profile::Column::CreatedAt.eq(value));
        }

        if let Some(start) = query.created_at_start {
            cond = cond.add(biz_showcase_profile::Column::CreatedAt.gte(start));
        }
        if let Some(end) = query.created_at_end {
            cond = cond.add(biz_showcase_profile::Column::CreatedAt.lte(end));
        }

        if let Some(value) = query.updated_at {
            cond = cond.add(biz_showcase_profile::Column::UpdatedAt.eq(value));
        }

        if let Some(start) = query.updated_at_start {
            cond = cond.add(biz_showcase_profile::Column::UpdatedAt.gte(start));
        }
        if let Some(end) = query.updated_at_end {
            cond = cond.add(biz_showcase_profile::Column::UpdatedAt.lte(end));
        }

        cond
    }
}
