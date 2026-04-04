pub use super::_entity::channel_model_price::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum ChannelModelPriceRelation {
    #[sea_orm(
        belongs_to = "super::channel::Entity",
        from = "Column::ChannelId",
        to = "super::channel::Column::Id"
    )]
    Channel,
    #[sea_orm(has_many = "super::channel_model_price_version::Entity")]
    Versions,
}

impl Related<super::channel::Entity> for Entity {
    fn to() -> RelationDef {
        ChannelModelPriceRelation::Channel.def()
    }
}

impl Related<super::channel_model_price_version::Entity> for Entity {
    fn to() -> RelationDef {
        ChannelModelPriceRelation::Versions.def()
    }
}

#[async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for ActiveModel {
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
