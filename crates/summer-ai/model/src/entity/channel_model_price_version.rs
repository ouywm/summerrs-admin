pub use super::_entity::channel_model_price_version::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum ChannelModelPriceVersionRelation {
    #[sea_orm(
        belongs_to = "super::channel_model_price::Entity",
        from = "Column::ChannelModelPriceId",
        to = "super::channel_model_price::Column::Id"
    )]
    Price,
}

impl Related<super::channel_model_price::Entity> for Entity {
    fn to() -> RelationDef {
        ChannelModelPriceVersionRelation::Price.def()
    }
}

#[async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            self.create_time = sea_orm::Set(chrono::Utc::now().fixed_offset());
        }
        Ok(self)
    }
}
