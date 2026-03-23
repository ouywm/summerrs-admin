pub mod dto;
pub mod entity;
pub mod views;
pub mod vo;

use sea_orm::{DatabaseConnection, DbErr};

// Raw entity sources live under `src/entity_gen` and are included by `src/entity/*`.
pub fn schema_registry_prefix() -> String {
    let crate_name = module_path!()
        .split("::")
        .next()
        .unwrap_or("summer_system_model");
    format!("{crate_name}::entity::*")
}

pub async fn sync_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    let prefix = schema_registry_prefix();
    db.get_schema_registry(&prefix).sync(db).await
}
