//! 系统后端 API 资源实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// HTTP method used by protected API resources.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ResourceMethod {
    #[serde(rename = "GET")]
    #[sea_orm(string_value = "GET")]
    Get,
    #[serde(rename = "POST")]
    #[sea_orm(string_value = "POST")]
    Post,
    #[serde(rename = "PUT")]
    #[sea_orm(string_value = "PUT")]
    Put,
    #[serde(rename = "PATCH")]
    #[sea_orm(string_value = "PATCH")]
    Patch,
    #[serde(rename = "DELETE")]
    #[sea_orm(string_value = "DELETE")]
    Delete,
}

impl ResourceMethod {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "resource")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 资源名称
    pub resource_name: String,
    /// 资源编码
    #[sea_orm(unique_key = "uk_sys_resource_code")]
    pub resource_code: String,
    /// HTTP 方法
    #[sea_orm(unique_key = "uk_sys_resource_method_path")]
    pub method: ResourceMethod,
    /// API 路径，支持 `{param}` 参数形式
    #[sea_orm(unique_key = "uk_sys_resource_method_path")]
    pub path: String,
    /// 资源说明
    pub description: String,
    /// 是否启用资源权限
    pub enabled: bool,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
    /// sys_resource -> sys_action_resource（一对多）
    #[sea_orm(has_many)]
    pub action_resources: HasMany<super::sys_action_resource::Entity>,
    /// sys_resource -> sys_menu（多对多，通过 sys_action_resource）
    #[sea_orm(has_many, via = "sys_action_resource")]
    pub actions: HasMany<super::sys_menu::Entity>,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    /// 保存前自动设置时间戳
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
