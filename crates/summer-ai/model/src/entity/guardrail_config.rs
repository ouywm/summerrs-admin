pub use super::_entity::guardrail_config::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum GuardrailConfigRelation {
    #[sea_orm(has_many = "super::guardrail_rule::Entity")]
    Rules,
}

impl Related<super::guardrail_rule::Entity> for Entity {
    fn to() -> RelationDef {
        GuardrailConfigRelation::Rules.def()
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
