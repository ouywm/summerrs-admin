use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 上游 API 风格 —— `ai.vendor.api_style` 列。决定运行时走哪个 adapter（dispatch 维度）。
///
/// 存储为 VARCHAR(64) 字符串值；新增 vendor 时若 `api_style` 命中已知变体则零代码。
/// 协议路由逻辑（ApiStyle + EndpointScope → AdapterKind）由 relay 层负责，详见
/// `relay::service::channel_store::resolve_adapter_kind`。
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(64))")]
pub enum ApiStyle {
    /// OpenAI 兼容（OpenAI 官方 / Azure / DeepSeek / Groq / 阿里 / 百度 / Mistral / ...）
    #[sea_orm(string_value = "openai-compatible")]
    OpenAiCompatible,
    /// Anthropic 原生 `/v1/messages`
    #[sea_orm(string_value = "anthropic-native")]
    AnthropicNative,
    /// Google Gemini `:generateContent`
    #[sea_orm(string_value = "gemini-native")]
    GeminiNative,
    /// Ollama 本地原生（兼容 OpenAI wire）
    #[sea_orm(string_value = "ollama-native")]
    OllamaNative,
}

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
    /// API 风格（决定运行时走哪个 adapter）
    pub api_style: ApiStyle,
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
