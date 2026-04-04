pub use super::_entity::alert_rule::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum AlertRuleRelation {
    #[sea_orm(has_many = "super::alert_event::Entity")]
    Events,
    #[sea_orm(has_many = "super::alert_silence::Entity")]
    Silences,
}

impl Related<super::alert_event::Entity> for Entity {
    fn to() -> RelationDef {
        AlertRuleRelation::Events.def()
    }
}

impl Related<super::alert_silence::Entity> for Entity {
    fn to() -> RelationDef {
        AlertRuleRelation::Silences.def()
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
