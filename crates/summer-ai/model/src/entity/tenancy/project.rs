use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=归档
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
pub enum ProjectStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 归档
    #[sea_orm(num_value = 3)]
    Archived = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "project")]
pub struct Model {
    /// 项目ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属组织ID
    pub organization_id: i64,
    /// 所属团队ID（可为空）
    pub team_id: Option<i64>,
    /// 项目负责人用户ID
    pub owner_user_id: i64,
    /// 项目编码（组织内唯一）
    pub project_code: String,
    /// 项目名称
    pub project_name: String,
    /// 可见性：private/internal/public
    pub visibility: String,
    /// 状态：1=启用 2=禁用 3=归档
    pub status: ProjectStatus,
    /// 项目总额度上限（0=不限制）
    pub quota_limit: i64,
    /// 项目累计已用额度
    pub used_quota: i64,
    /// 项目日额度上限
    pub daily_quota_limit: i64,
    /// 项目月额度上限
    pub monthly_quota_limit: i64,
    /// 项目累计请求数
    pub request_count: i64,
    /// 项目级设置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub settings: serde_json::Value,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
