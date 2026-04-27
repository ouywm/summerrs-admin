use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 渠道类型（admin 分组用）：1=OpenAI 2=Anthropic 3=Azure 4=Baidu 5=Ali 6=Gemini 7=Ollama
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
pub enum ChannelType {
    /// OpenAI
    #[sea_orm(num_value = 1)]
    OpenAi = 1,
    /// Anthropic
    #[sea_orm(num_value = 2)]
    Anthropic = 2,
    /// Azure
    #[sea_orm(num_value = 3)]
    Azure = 3,
    /// Baidu
    #[sea_orm(num_value = 4)]
    Baidu = 4,
    /// Ali
    #[sea_orm(num_value = 5)]
    Ali = 5,
    /// Gemini
    #[sea_orm(num_value = 6)]
    Gemini = 6,
    /// Ollama
    #[sea_orm(num_value = 7)]
    Ollama = 7,
}

/// 状态：1=启用 2=手动禁用 3=自动禁用 4=归档
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
pub enum ChannelStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 手动禁用
    #[sea_orm(num_value = 2)]
    ManualDisabled = 2,
    /// 自动禁用
    #[sea_orm(num_value = 3)]
    AutoDisabled = 3,
    /// 归档
    #[sea_orm(num_value = 4)]
    Archived = 4,
}

/// 健康状态：0=未知 1=健康 2=警告 3=异常
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
pub enum ChannelLastHealthStatus {
    /// 未知
    #[sea_orm(num_value = 0)]
    Unknown = 0,
    /// 健康
    #[sea_orm(num_value = 1)]
    Healthy = 1,
    /// 警告
    #[sea_orm(num_value = 2)]
    Warning = 2,
    /// 异常
    #[sea_orm(num_value = 3)]
    Unhealthy = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel")]
pub struct Model {
    /// 渠道ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道名称（如：OpenAI官方、DeepSeek公网代理）
    pub name: String,
    /// 渠道类型：1=OpenAI 3=Anthropic 14=Azure 15=Baidu 17=Ali 24=Gemini 28=Ollama
    pub channel_type: ChannelType,
    /// 供应商编码（对应 ai.vendor.vendor_code）
    pub vendor_code: String,
    /// 上游 API 基础地址
    pub base_url: String,
    /// 状态：1=启用 2=手动禁用 3=自动禁用 4=归档
    pub status: ChannelStatus,
    /// 支持的模型列表（JSON 数组，如 ["gpt-4o","gpt-4o-mini"]）
    #[sea_orm(column_type = "JsonBinary")]
    pub models: serde_json::Value,
    /// 模型名映射（JSON，如 {"gpt-4": "gpt-4-turbo"}）
    #[sea_orm(column_type = "JsonBinary")]
    pub model_mapping: serde_json::Value,
    /// 渠道分组（用户分组命中后按此分组做路由）
    pub channel_group: String,
    /// 该渠道支持的 endpoint 范围（JSON 数组，如 ["chat","responses","embeddings"]）
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// 渠道能力标签（JSON 数组，如 ["vision","tool_call","reasoning"]）
    #[sea_orm(column_type = "JsonBinary")]
    pub capabilities: serde_json::Value,
    /// 路由权重（同优先级内加权随机）
    pub weight: i32,
    /// 路由优先级（越大越优先）
    pub priority: i32,
    /// 渠道扩展配置（JSON，如 organization、region、headers、safety 等）
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    /// 是否启用自动禁用
    pub auto_ban: bool,
    /// 测速使用的模型
    pub test_model: String,
    /// 累计已消耗配额
    pub used_quota: i64,
    /// 渠道/供应商维度余额快照
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance: BigDecimal,
    /// 余额最后更新时间
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    /// 最近一次测速响应时间（毫秒）
    pub response_time: i32,
    /// 近期成功率（0-100）
    #[sea_orm(column_type = "Decimal(Some((8, 4)))")]
    pub success_rate: BigDecimal,
    /// 连续失败次数
    pub failure_streak: i32,
    /// 最近一次被实际选中的时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 最近一次错误时间
    pub last_error_at: Option<DateTimeWithTimeZone>,
    /// 最近一次错误码
    pub last_error_code: String,
    /// 最近一次错误摘要
    #[sea_orm(column_type = "Text")]
    pub last_error_message: String,
    /// 健康状态：0=未知 1=健康 2=警告 3=异常
    pub last_health_status: ChannelLastHealthStatus,
    /// 软删除时间
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

    /// channel -> channel_account（一对多）
    #[sea_orm(has_many)]
    /// channel accounts
    pub channel_accounts: HasMany<super::channel_account::Entity>,

    /// channel -> ability（一对多）
    #[sea_orm(has_many)]
    /// abilities
    pub abilities: HasMany<super::ability::Entity>,
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

impl Entity {
    pub async fn find_enabled_undeleted_by_ids<C>(
        db: &C,
        channel_ids: &[i64],
    ) -> Result<Vec<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        if channel_ids.is_empty() {
            return Ok(Vec::new());
        }

        Self::find()
            .filter(Column::Id.is_in(channel_ids.to_vec()))
            .filter(Column::DeletedAt.is_null())
            .filter(Column::Status.eq(ChannelStatus::Enabled))
            .all(db)
            .await
    }
}

impl Model {
    pub fn resolve_upstream_model(&self, requested_model: &str) -> String {
        self.model_mapping
            .get(requested_model)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(requested_model)
            .to_string()
    }
}
