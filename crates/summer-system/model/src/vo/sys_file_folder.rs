//! 文件夹 VO（文件中心）

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::sys_file_folder;

/// 文件夹树节点
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileFolderTreeVo {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub sort: i32,
    pub file_count: i64,
    #[schemars(skip)]
    pub children: Vec<FileFolderTreeVo>,
}

/// 文件夹详情/列表项
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileFolderVo {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub sort: i32,
    pub file_count: i64,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl FileFolderVo {
    pub fn from_model(m: sys_file_folder::Model, file_count: i64) -> Self {
        Self {
            id: m.id,
            parent_id: m.parent_id,
            name: m.name,
            slug: m.slug,
            visibility: m.visibility,
            sort: m.sort,
            file_count,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
