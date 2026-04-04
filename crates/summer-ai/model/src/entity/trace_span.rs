pub use super::_entity::trace_span::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum TraceSpanRelation {
    #[sea_orm(
        belongs_to = "super::trace::Entity",
        from = "Column::TraceId",
        to = "super::trace::Column::Id"
    )]
    Trace,
}

impl Related<super::trace::Entity> for Entity {
    fn to() -> RelationDef {
        TraceSpanRelation::Trace.def()
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
