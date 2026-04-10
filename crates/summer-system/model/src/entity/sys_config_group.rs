//! 系统参数分组实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "config_group")]
pub struct Model {
    /// 分组ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 分组名称
    pub group_name: String,
    /// 分组编码（唯一标识，如 basic/security）
    #[sea_orm(unique)]
    pub group_code: String,
    /// 分组排序，值越小越靠前
    pub group_sort: i32,
    /// 是否启用
    pub enabled: bool,
    /// 是否系统内置（防止误删）
    pub is_system: bool,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTime,
    /// sys_config_group -> sys_config（一对多）
    #[sea_orm(has_many)]
    pub configs: HasMany<super::sys_config::Entity>,
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
