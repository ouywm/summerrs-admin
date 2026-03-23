#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::sys_login_log::ActiveModel {
    /// 保存前自动设置时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Local::now().naive_local();
            self.create_time = sea_orm::Set(now);
            if self.login_time.is_not_set() {
                self.login_time = sea_orm::Set(now);
            }
        }
        Ok(self)
    }
}
