pub use sea_orm_migration::prelude::MigratorTrait;

mod m20260612_000001_seed_system_data;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn sea_orm_migration::prelude::MigrationTrait>> {
        vec![Box::new(m20260612_000001_seed_system_data::Migration)]
    }
}
