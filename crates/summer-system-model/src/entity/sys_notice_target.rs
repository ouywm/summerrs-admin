#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::sys_notice_target::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
