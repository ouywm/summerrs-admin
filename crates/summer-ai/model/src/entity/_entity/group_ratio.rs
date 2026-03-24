//! AI 分组倍率实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "group_ratio")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 分组编码
    #[sea_orm(unique)]
    pub group_code: String,
    /// 分组名称
    pub group_name: String,
    /// 倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub ratio: BigDecimal,
    /// 是否启用
    pub enabled: bool,
    /// 模型白名单
    #[sea_orm(column_type = "JsonBinary")]
    pub model_whitelist: serde_json::Value,
    /// 模型黑名单
    #[sea_orm(column_type = "JsonBinary")]
    pub model_blacklist: serde_json::Value,
    /// 端点作用域
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// 回退分组编码
    pub fallback_group_code: String,
    /// 策略配置
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
