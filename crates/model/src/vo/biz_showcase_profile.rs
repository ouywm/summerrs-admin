//! Generated admin VO skeleton.


use common::serde_utils::datetime_format;

use schemars::JsonSchema;
use serde::Serialize;

use sea_orm::prelude::Decimal;


use crate::entity::biz_showcase_profile;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowcaseProfileVo {



    /// 主键


    pub id: i64,



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


    pub contact_gender: i16,



    /// 联系电话


    pub contact_phone: Option<String>,



    /// 联系邮箱


    pub contact_email: Option<String>,



    /// 官网链接


    pub official_url: Option<String>,



    /// 状态


    pub status: i16,



    /// 推荐


    pub featured: bool,



    /// 优先级


    pub priority: i32,


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


    #[serde(serialize_with = "datetime_format::serialize")]

    pub created_at: chrono::NaiveDateTime,



    /// 更新时间


    #[serde(serialize_with = "datetime_format::serialize")]

    pub updated_at: chrono::NaiveDateTime,

}

impl From<biz_showcase_profile::Model> for ShowcaseProfileVo {
    fn from(model: biz_showcase_profile::Model) -> Self {
        Self {

            id: model.id,

            showcase_code: model.showcase_code,

            title: model.title,

            avatar: model.avatar,

            cover_image: model.cover_image,

            contact_name: model.contact_name,

            contact_gender: model.contact_gender,

            contact_phone: model.contact_phone,

            contact_email: model.contact_email,

            official_url: model.official_url,

            status: model.status,

            featured: model.featured,

            priority: model.priority,

            score: model.score,

            publish_date: model.publish_date,

            launch_at: model.launch_at,

            service_time: model.service_time,

            attachment_url: model.attachment_url,

            description: model.description,

            extra_notes: model.extra_notes,

            metadata: model.metadata,

            created_at: model.created_at,

            updated_at: model.updated_at,

        }
    }
}
