pub use super::_entity::request_execution::*;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum RequestExecutionRelation {
    #[sea_orm(
        belongs_to = "super::request::Entity",
        from = "Column::AiRequestId",
        to = "super::request::Column::Id"
    )]
    Request,
}

impl Related<super::request::Entity> for Entity {
    fn to() -> RelationDef {
        RequestExecutionRelation::Request.def()
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
