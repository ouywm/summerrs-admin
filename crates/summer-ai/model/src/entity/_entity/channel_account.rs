//! AI 渠道账号实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 账号状态（1=启用, 2=禁用, 3=耗尽, 4=过期, 5=冷却中）
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
pub enum AccountStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 耗尽
    #[sea_orm(num_value = 3)]
    Exhausted = 3,
    /// 过期
    #[sea_orm(num_value = 4)]
    Expired = 4,
    /// 冷却中
    #[sea_orm(num_value = 5)]
    Cooling = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_account")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道 ID
    pub channel_id: i64,
    /// 账号名称
    pub name: String,
    /// 凭证类型
    pub credential_type: String,
    /// 凭证信息
    #[sea_orm(column_type = "JsonBinary")]
    pub credentials: serde_json::Value,
    /// 密钥引用
    pub secret_ref: String,
    /// 账号状态
    pub status: AccountStatus,
    /// 是否可调度
    pub schedulable: bool,
    /// 优先级
    pub priority: i32,
    /// 权重
    pub weight: i32,
    /// 费率倍数
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub rate_multiplier: BigDecimal,
    /// 并发限制
    pub concurrency_limit: i32,
    /// 额度上限
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_limit: BigDecimal,
    /// 已用额度
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_used: BigDecimal,
    /// 余额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance: BigDecimal,
    /// 余额更新时间
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    /// 响应时间（毫秒）
    pub response_time: i32,
    /// 连续失败次数
    pub failure_streak: i32,
    /// 最后使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 最后错误时间
    pub last_error_at: Option<DateTimeWithTimeZone>,
    /// 最后错误码
    pub last_error_code: String,
    /// 最后错误信息
    #[sea_orm(column_type = "Text", nullable)]
    pub last_error_message: Option<String>,
    /// 限速恢复时间
    pub rate_limited_until: Option<DateTimeWithTimeZone>,
    /// 过载恢复时间
    pub overload_until: Option<DateTimeWithTimeZone>,
    /// 过期时间
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// 测试模型
    pub test_model: String,
    /// 测试时间
    pub test_time: Option<DateTimeWithTimeZone>,
    /// 扩展信息
    #[sea_orm(column_type = "JsonBinary")]
    pub extra: serde_json::Value,
    /// 删除时间（软删除）
    pub deleted_at: Option<DateTimeWithTimeZone>,
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
    /// 关联渠道（多对一，逻辑关联 ai.channel.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "channel_id", to = "id", skip_fk)]
    pub channel: Option<super::channel::Entity>,
}
