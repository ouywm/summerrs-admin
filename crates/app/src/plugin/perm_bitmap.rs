//! 权限位图初始化插件：启动时从 DB 加载 PermissionMap 并注入 SessionManager

use model::entity::sys_menu;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_auth::{PermissionMap, SessionManager};

use crate::plugin::sea_orm::DbConn;

pub struct PermBitmapPlugin;

#[async_trait]
impl Plugin for PermBitmapPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DbConn = app
            .get_component::<DbConn>()
            .expect("DbConn 未找到，请确保 SeaOrmPlugin 在 PermBitmapPlugin 之前注册");

        let auth: SessionManager = app
            .get_component::<SessionManager>()
            .expect("SessionManager 未找到，请确保 SummerAuthPlugin 在 PermBitmapPlugin 之前注册");

        let menus = sys_menu::Entity::find()
            .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Button))
            .filter(sys_menu::Column::Enabled.eq(true))
            .filter(sys_menu::Column::BitPosition.is_not_null())
            .all(&db)
            .await
            .expect("加载权限位图映射失败");

        let mappings: Vec<(String, u32)> = menus
            .into_iter()
            .filter(|m| !m.auth_mark.is_empty())
            .map(|m| (m.auth_mark, m.bit_position.unwrap() as u32))
            .collect();

        if mappings.is_empty() {
            tracing::info!("No permission bitmap mappings found, JWT will use permissions array");
        } else {
            tracing::info!(
                "Loaded {} permission bitmap mappings",
                mappings.len()
            );
            auth.set_permission_map(PermissionMap::new(mappings));
        }
    }

    fn name(&self) -> &str {
        "perm-bitmap"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["sea-orm", "summer_auth::SummerAuthPlugin"]
    }
}
