use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 计费模式：1=按 token 2=按请求 3=按图片/音频/视频单位
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
pub enum ChannelModelPriceBillingMode {
    /// 按 token
    #[sea_orm(num_value = 1)]
    ByToken = 1,
    /// 按请求
    #[sea_orm(num_value = 2)]
    ByRequest = 2,
    /// 按图片/音频/视频单位
    #[sea_orm(num_value = 3)]
    ByMediaUnit = 3,
}

/// 状态：1=启用 2=停用
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
pub enum ChannelModelPriceStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 停用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_model_price")]
pub struct Model {
    /// 价格ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道ID
    pub channel_id: i64,
    /// 模型名
    pub model_name: String,
    /// 计费模式：1=按 token 2=按请求 3=按图片/音频/视频单位
    pub billing_mode: ChannelModelPriceBillingMode,
    /// 价格货币
    pub currency: String,
    /// 价格配置 JSON（如 input/output/cache/reasoning 等单价）
    #[sea_orm(column_type = "JsonBinary")]
    pub price_config: serde_json::Value,
    /// 价格快照引用ID，记账时落到 ai.log.price_reference
    pub reference_id: String,
    /// 状态：1=启用 2=停用
    pub status: ChannelModelPriceStatus,
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
