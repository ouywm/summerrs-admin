//! AI 供应商表实体
//! 对标 new-api vendor / one-hub model_ownedby

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vendor")]
pub struct Model {
    /// 供应商 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 供应商编码（唯一）
    #[sea_orm(unique)]
    pub vendor_code: String,
    /// 供应商名称
    pub vendor_name: String,
    /// API 风格（openai-compatible / anthropic-native / gemini-native）
    pub api_style: String,
    /// 图标 URL 或 SVG
    pub icon: String,
    /// 供应商简介
    pub description: String,
    /// 官方默认 API 地址
    pub base_url: String,
    /// 官方文档地址
    pub doc_url: String,
    /// 供应商扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 排序（越小越靠前）
    pub vendor_sort: i32,
    /// 是否启用
    pub enabled: bool,
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
