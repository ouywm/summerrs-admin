pub mod dto;
pub mod entity;
pub mod vo;

use sea_orm::{DatabaseConnection, DbErr};

pub fn schema_registry_prefix() -> String {
    let crate_name = module_path!()
        .split("::")
        .next()
        .unwrap_or("summer_ai_model");
    format!("{crate_name}::entity::*")
}

pub async fn sync_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    let prefix = schema_registry_prefix();
    db.get_schema_registry(&prefix).sync(db).await
}
