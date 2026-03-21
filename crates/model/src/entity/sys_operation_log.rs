//! 系统操作日志实体

use schemars::JsonSchema;
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 操作类型（0=其他, 1=新增, 2=修改, 3=删除, 4=查询, 5=导出, 6=导入, 7=授权）
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
pub enum BusinessType {
    /// 其他
    #[sea_orm(num_value = 0)]
    Other = 0,
    /// 新增
    #[sea_orm(num_value = 1)]
    Create = 1,
    /// 修改
    #[sea_orm(num_value = 2)]
    Update = 2,
    /// 删除
    #[sea_orm(num_value = 3)]
    Delete = 3,
    /// 查询
    #[sea_orm(num_value = 4)]
    Query = 4,
    /// 导出
    #[sea_orm(num_value = 5)]
    Export = 5,
    /// 导入
    #[sea_orm(num_value = 6)]
    Import = 6,
    /// 授权
    #[sea_orm(num_value = 7)]
    Auth = 7,
}

/// 操作状态（1=成功, 2=失败, 3=异常）
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
pub enum OperationStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Success = 1,
    /// 失败
    #[sea_orm(num_value = 2)]
    Failed = 2,
    /// 异常
    #[sea_orm(num_value = 3)]
    Exception = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "operation_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: Option<i64>,
    pub user_name: Option<String>,
    pub module: String,
    pub action: String,
    pub business_type: BusinessType,
    pub request_method: String,
    pub request_url: String,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub request_params: Option<Json>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub response_body: Option<Json>,
    pub response_code: i16,
    #[sea_orm(column_type = "Inet", nullable)]
    pub client_ip: Option<IpNetwork>,
    pub ip_location: Option<String>,
    pub user_agent: Option<String>,
    pub status: OperationStatus,
    pub error_msg: Option<String>,
    pub duration: i64,
    pub create_time: DateTime,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            self.create_time = Set(chrono::Local::now().naive_local());
        }
        Ok(self)
    }
}
