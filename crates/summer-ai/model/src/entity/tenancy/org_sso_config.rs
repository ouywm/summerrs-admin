//! AI 组织 SSO 配置表
//! 对应 sql/ai/org_sso_config.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=测试
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
pub enum OrgSsoConfigStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 测试
    #[sea_orm(num_value = 3)]
    Testing = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "org_sso_config")]
pub struct Model {
    /// SSO 配置ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// SSO 提供方编码（组织内唯一）
    pub provider_code: String,
    /// SSO 提供方名称
    pub provider_name: String,
    /// 协议类型：oidc/saml
    pub protocol_type: String,
    /// OIDC/SAML issuer
    pub issuer: String,
    /// 登录入口地址
    pub entrypoint_url: String,
    /// 回调地址
    pub callback_url: String,
    /// SAML Entity ID 或应用标识
    pub entity_id: String,
    /// Audience
    pub audience: String,
    /// 客户端 ID
    pub client_id: String,
    /// 客户端密钥引用
    pub client_secret_ref: String,
    /// 证书内容
    #[sea_orm(column_type = "Text")]
    pub certificate_pem: String,
    /// 允许自动接入的域名列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub allowed_domains: serde_json::Value,
    /// 自动开通时默认角色
    pub default_role_code: String,
    /// 是否启用 JIT 登录开通
    pub jit_enabled: bool,
    /// 是否自动创建/同步成员
    pub auto_provision: bool,
    /// 是否默认 SSO 配置
    pub is_default: bool,
    /// 状态：1=启用 2=禁用 3=测试
    pub status: OrgSsoConfigStatus,
    /// 最近使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 协议配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
