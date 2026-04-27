use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "ability")]
pub struct Model {
    /// 能力ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道分组（对应 ai.channel.channel_group）
    pub channel_group: String,
    /// endpoint 范围：chat/responses/embeddings/images/audio/batches 等
    pub endpoint_scope: String,
    /// 模型标识（请求侧模型名）
    pub model: String,
    /// 渠道ID（ai.channel.id）
    pub channel_id: i64,
    /// 是否启用
    pub enabled: bool,
    /// 路由优先级（覆盖渠道默认值）
    pub priority: i32,
    /// 路由权重（覆盖渠道默认值）
    pub weight: i32,
    /// 能力级路由扩展配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub route_config: serde_json::Value,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,

    /// 关联渠道（多对一）
    #[sea_orm(belongs_to, from = "channel_id", to = "id", skip_fk)]
    pub channel: Option<super::channel::Entity>,
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
    pub async fn find_enabled_route_candidates<C>(
        db: &C,
        channel_group: &str,
        endpoint_scope: &str,
        requested_model: &str,
    ) -> Result<Vec<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        Self::find()
            .filter(Column::ChannelGroup.eq(channel_group))
            .filter(Column::EndpointScope.eq(endpoint_scope))
            .filter(Column::Model.eq(requested_model.to_string()))
            .filter(Column::Enabled.eq(true))
            .order_by_desc(Column::Priority)
            .order_by_desc(Column::Weight)
            .order_by_desc(Column::ChannelId)
            .all(db)
            .await
    }
}
