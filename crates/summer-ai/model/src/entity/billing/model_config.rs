use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 模型类型：1=chat 2=embedding 3=image 4=audio 5=reasoning
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
pub enum ModelConfigType {
    /// chat
    #[sea_orm(num_value = 1)]
    Chat = 1,
    /// embedding
    #[sea_orm(num_value = 2)]
    Embedding = 2,
    /// image
    #[sea_orm(num_value = 3)]
    Image = 3,
    /// audio
    #[sea_orm(num_value = 4)]
    Audio = 4,
    /// reasoning
    #[sea_orm(num_value = 5)]
    Reasoning = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "model_config")]
pub struct Model {
    /// 配置ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 模型标识（唯一）
    pub model_name: String,
    /// 模型显示名称
    pub display_name: String,
    /// 模型类型：1=chat 2=embedding 3=image 4=audio 5=reasoning
    pub model_type: ModelConfigType,
    /// 供应商编码（对应 ai.vendor.vendor_code）
    pub vendor_code: String,
    /// 支持的 endpoint 范围（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub supported_endpoints: serde_json::Value,
    /// 输入 token 计费倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub input_ratio: BigDecimal,
    /// 输出 token 计费倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub output_ratio: BigDecimal,
    /// 缓存命中 token 计费倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub cached_input_ratio: BigDecimal,
    /// 推理 token 计费倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub reasoning_ratio: BigDecimal,
    /// 模型能力标签（JSON 数组，如 ["vision","tool_call"]）
    #[sea_orm(column_type = "JsonBinary")]
    pub capabilities: serde_json::Value,
    /// 最大上下文长度
    pub max_context: i32,
    /// 默认成本货币
    pub currency: String,
    /// 默认倍率生效时间
    pub effective_from: Option<DateTimeWithTimeZone>,
    /// 模型补充元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
