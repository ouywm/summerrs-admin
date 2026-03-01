//! 系统登录日志实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::IpNetwork;
use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 登录状态（1: 成功, 2: 失败）
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
pub enum LoginStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Success = 1,
    /// 失败
    #[sea_orm(num_value = 2)]
    Failed = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_login_log")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 用户名
    pub user_name: String,
    /// 登录时间
    pub login_time: DateTime,
    /// 登录IP
    #[sea_orm(column_type = "Inet")]
    pub login_ip: IpNetwork,
    /// 登录地理位置
    pub login_location: String,
    /// 浏览器User-Agent
    pub user_agent: String,
    /// 浏览器
    pub browser: String,
    /// 浏览器版本
    pub browser_version: String,
    /// 操作系统
    pub os: String,
    /// 操作系统版本
    pub os_version: String,
    /// 设备类型
    pub device: String,
    /// 登录状态
    pub status: LoginStatus,
    /// 失败原因
    pub fail_reason: String,
    /// 创建时间
    pub create_time: DateTime,

    /// 关联用户（多对一）
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::sys_user::Entity>,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 保存前自动设置时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            let now = chrono::Local::now().naive_local();
            self.create_time = Set(now);
            if self.login_time.is_not_set() {
                self.login_time = Set(now);
            }
        }
        Ok(self)
    }
}
