pub use super::_entity::channel::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum ChannelRelation {
    #[sea_orm(has_many = "super::channel_account::Entity")]
    ChannelAccounts,
    #[sea_orm(has_many = "super::ability::Entity")]
    Abilities,
}

impl Related<super::channel_account::Entity> for Entity {
    fn to() -> RelationDef {
        ChannelRelation::ChannelAccounts.def()
    }
}

impl Related<super::ability::Entity> for Entity {
    fn to() -> RelationDef {
        ChannelRelation::Abilities.def()
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
