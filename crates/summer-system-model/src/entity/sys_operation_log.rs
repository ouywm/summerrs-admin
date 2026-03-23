#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::sys_operation_log::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            self.create_time = sea_orm::Set(chrono::Local::now().naive_local());
        }
        Ok(self)
    }
}
