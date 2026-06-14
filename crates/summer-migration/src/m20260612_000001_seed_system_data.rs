use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

const SYSTEM_SEED_SQL: &str = include_str!("../../../sql/sys/seed/system_data.sql");

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(SYSTEM_SEED_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
