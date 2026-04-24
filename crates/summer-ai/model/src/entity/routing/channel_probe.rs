use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 检查类型：1=手动测速 2=定时检查 3=故障后自动恢复
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
pub enum ChannelProbeType {
    /// 手动测速
    #[sea_orm(num_value = 1)]
    ManualProbe = 1,
    /// 定时检查
    #[sea_orm(num_value = 2)]
    ScheduledCheck = 2,
    /// 故障后自动恢复
    #[sea_orm(num_value = 3)]
    AutoRecovery = 3,
}

/// 检查结果：1=成功 2=失败 3=超时
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
pub enum ChannelProbeStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Succeeded = 1,
    /// 失败
    #[sea_orm(num_value = 2)]
    Failed = 2,
    /// 超时
    #[sea_orm(num_value = 3)]
    Timeout = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_probe")]
pub struct Model {
    /// 检查ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道ID
    pub channel_id: i64,
    /// 账号ID（0 表示只测渠道级）
    pub account_id: i64,
    /// 检查请求ID
    pub request_id: String,
    /// 检查类型：1=手动测速 2=定时检查 3=故障后自动恢复
    pub probe_type: ChannelProbeType,
    /// 测试模型
    pub test_model: String,
    /// 检查结果：1=成功 2=失败 3=超时
    pub status: ChannelProbeStatus,
    /// 总响应时间（毫秒）
    pub response_time: i32,
    /// 首 token 时间（毫秒）
    pub first_token_time: i32,
    /// HTTP 状态码
    pub status_code: i32,
    /// 错误摘要
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 测试请求体
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub request_body: Option<serde_json::Value>,
    /// 测试响应体摘要
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub response_body: Option<serde_json::Value>,
    /// 检查时间
    pub create_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Utc::now().fixed_offset();
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
