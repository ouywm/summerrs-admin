//! 权限位图初始化插件：启动时从 DB 加载 PermissionMap 并注入 SessionManager

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_auth::{PermissionMap, SessionManager};
use summer_domain::menu::MenuDomainService;

use summer_sea_orm::DbConn;

pub struct PermBitmapPlugin;

#[async_trait]
impl Plugin for PermBitmapPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DbConn = app
            .get_component::<DbConn>()
            .expect("DbConn 未找到，请确保 SeaOrmPlugin 在 PermBitmapPlugin 之前注册");

        let auth: SessionManager = app
            .get_component::<SessionManager>()
            .expect(
                "SessionManager 未找到，请确保 summer_auth::SummerAuthPlugin 在 PermBitmapPlugin 之前注册",
            );

        let mappings = MenuDomainService::new(db)
            .load_permission_mappings()
            .await
            .expect("加载权限位图映射失败");

        if mappings.is_empty() {
            tracing::info!("No permission bitmap mappings found, JWT will use permissions array");
        } else {
            tracing::info!("Loaded {} permission bitmap mappings", mappings.len());
            auth.set_permission_map(PermissionMap::new(mappings));
        }
    }

    fn name(&self) -> &str {
        "perm-bitmap"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin", "summer_auth::SummerAuthPlugin"]
    }
}
