pub use super::_entity::trace::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum TraceRelation {
    #[sea_orm(has_many = "super::trace_span::Entity")]
    Spans,
}

impl Related<super::trace_span::Entity> for Entity {
    fn to() -> RelationDef {
        TraceRelation::Spans.def()
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
