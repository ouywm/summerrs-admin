//! AI 域名验证表实体
//! 企业域归属验证记录

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 域名验证状态（1=待验证 2=已验证 3=失败 4=过期）
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
pub enum DomainVerificationStatus {
    /// 待验证
    #[sea_orm(num_value = 1)]
    Pending = 1,
    /// 已验证
    #[sea_orm(num_value = 2)]
    Verified = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 过期
    #[sea_orm(num_value = 4)]
    Expired = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "domain_verification")]
pub struct Model {
    /// 域名验证ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 关联 SSO 配置ID
    pub sso_config_id: Option<i64>,
    /// 待验证域名
    pub domain_name: String,
    /// 验证方式：dns_txt/http_file/email
    pub verification_type: String,
    /// 验证令牌
    pub verification_token: String,
    /// DNS 记录名
    pub dns_record_name: String,
    /// DNS 记录类型
    pub dns_record_type: String,
    /// DNS 记录值
    pub dns_record_value: String,
    /// HTTP 校验文件路径
    pub http_file_path: String,
    /// HTTP 校验文件内容
    pub http_file_content: String,
    /// 状态
    pub status: DomainVerificationStatus,
    /// 尝试次数
    pub attempt_count: i32,
    /// 最后校验时间
    pub last_checked_at: Option<DateTimeWithTimeZone>,
    /// 验证成功时间
    pub verified_at: Option<DateTimeWithTimeZone>,
    /// 验证过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
