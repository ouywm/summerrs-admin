//! AI 分组倍率与策略表（同一分组的价格、模型权限和兜底策略）
//! 对应 sql/ai/group_ratio.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "group_ratio")]
pub struct Model {
    /// 分组ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 分组编码（唯一）
    pub group_code: String,
    /// 分组名称
    pub group_name: String,
    /// 计费倍率（1.0=标准价）
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub ratio: BigDecimal,
    /// 是否启用
    pub enabled: bool,
    /// 分组级允许模型列表（JSON 数组，空=不限制）
    #[sea_orm(column_type = "JsonBinary")]
    pub model_whitelist: serde_json::Value,
    /// 分组级禁用模型列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub model_blacklist: serde_json::Value,
    /// 分组级允许 endpoint 范围（JSON 数组，空=不限制）
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// 请求不满足规则时的降级目标分组
    pub fallback_group_code: String,
    /// 组策略 JSON（如固定渠道、灰度开关、客户端限制等）
    #[sea_orm(column_type = "JsonBinary")]
    pub policy: serde_json::Value,
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
