//! 字典类型实体

use schemars::JsonSchema;
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 字典状态（1: 启用, 2: 禁用）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum DictStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_dict_type")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 字典名称
    pub dict_name: String,
    /// 字典类型编码（唯一）
    #[sea_orm(unique)]
    pub dict_type: String,
    /// 状态
    pub status: DictStatus,
    /// 是否系统内置
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
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 保存前自动设置时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = Set(now);
        if insert {
            self.create_time = Set(now);
        }
        Ok(self)
    }
}
