//! 系统文件夹实体（文件中心）

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "file_folder")]
pub struct Model {
    /// 主键ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 父级文件夹ID（0表示根）
    pub parent_id: i64,
    /// 文件夹名称
    pub name: String,
    /// 文件夹slug（同级唯一，可用于路由/检索）
    pub slug: String,
    /// 可见性（如 PUBLIC/PRIVATE）
    pub visibility: String,
    /// 排序（数值越小越靠前）
    pub sort: i32,
    /// 文件数量聚合字段（可选冗余）
    pub file_count: i64,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
