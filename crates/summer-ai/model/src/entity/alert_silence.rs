pub use super::_entity::alert_silence::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum AlertSilenceRelation {
    #[sea_orm(
        belongs_to = "super::alert_rule::Entity",
        from = "Column::AlertRuleId",
        to = "super::alert_rule::Column::Id"
    )]
    Rule,
}

impl Related<super::alert_rule::Entity> for Entity {
    fn to() -> RelationDef {
        AlertSilenceRelation::Rule.def()
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
