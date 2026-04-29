//! 文件夹 DTO（文件中心）

use schemars::JsonSchema;
use sea_orm::{NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_file_folder;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileFolderDto {
    /// 父级文件夹ID（0表示根）
    pub parent_id: Option<i64>,
    /// 文件夹名称
    #[validate(length(min = 1, max = 128, message = "文件夹名称长度必须在1-128之间"))]
    pub name: String,
    /// slug（同级唯一）
    #[validate(length(min = 1, max = 128, message = "slug长度必须在1-128之间"))]
    pub slug: String,
    /// 可见性（如 PUBLIC/PRIVATE）
    #[validate(length(min = 1, max = 32, message = "可见性不能为空"))]
    pub visibility: Option<String>,
    /// 排序
    pub sort: Option<i32>,
}

impl From<CreateFileFolderDto> for sys_file_folder::ActiveModel {
    fn from(dto: CreateFileFolderDto) -> Self {
        Self {
            id: NotSet,
            parent_id: Set(dto.parent_id.unwrap_or(0)),
            name: Set(dto.name),
            slug: Set(dto.slug),
            visibility: Set(dto.visibility.unwrap_or_else(|| "PRIVATE".to_string())),
            sort: Set(dto.sort.unwrap_or(0)),
            create_time: NotSet,
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFileFolderDto {
    #[validate(length(min = 1, max = 128, message = "文件夹名称长度必须在1-128之间"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 128, message = "slug长度必须在1-128之间"))]
    pub slug: Option<String>,
    #[validate(length(min = 1, max = 32, message = "可见性不能为空"))]
    pub visibility: Option<String>,
    pub sort: Option<i32>,
}

impl UpdateFileFolderDto {
    pub fn apply_to(self, active: &mut sys_file_folder::ActiveModel) {
        if let Some(name) = self.name {
            active.name = Set(name);
        }
        if let Some(slug) = self.slug {
            active.slug = Set(slug);
        }
        if let Some(visibility) = self.visibility {
            active.visibility = Set(visibility);
        }
        if let Some(sort) = self.sort {
            active.sort = Set(sort);
        }
    }
}
