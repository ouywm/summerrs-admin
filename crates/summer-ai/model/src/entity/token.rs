pub use super::_entity::token::*;

impl TokenStatus {
    pub fn label(self) -> &'static str {
        match self {
            TokenStatus::Enabled => "enabled",
            TokenStatus::Disabled => "disabled",
            TokenStatus::Expired => "expired",
            TokenStatus::Exhausted => "exhausted",
        }
    }
}

impl std::fmt::Display for TokenStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
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
