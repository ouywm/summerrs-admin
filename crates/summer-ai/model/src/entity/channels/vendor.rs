//! AI 供应商表（模型供应商元数据，用于展示与分类）
//! 对应 sql/ai/vendor.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vendor")]
pub struct Model {
    /// 供应商ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 供应商编码（唯一）
    pub vendor_code: String,
    /// 供应商名称
    pub vendor_name: String,
    /// API 风格（如 openai-compatible / anthropic-native / gemini-native）
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
