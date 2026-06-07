use std::collections::HashMap;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_auth::{ResourcePermissionPolicy, ResourcePermissionRegistry, ResourcePermissionRule};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_system_model::entity::{sys_action_resource, sys_menu, sys_resource};

pub struct ResourcePermissionPlugin;

#[async_trait]
impl Plugin for ResourcePermissionPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DbConn = app.get_component::<DbConn>().expect(
            "DbConn component is missing; register SeaOrmPlugin before ResourcePermissionPlugin",
        );

        let policy = match load_resource_permission_policy(&db).await {
            Ok(policy) => policy,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "resource permission policy unavailable, falling back to allow-all policy"
                );
                ResourcePermissionPolicy::default()
            }
        };

        tracing::info!("Loaded resource permission policy");
        app.add_component(ResourcePermissionRegistry::new(policy));
    }

    fn name(&self) -> &str {
        "resource-permission"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

pub async fn load_resource_permission_policy(db: &DbConn) -> ApiResult<ResourcePermissionPolicy> {
    let resources = sys_resource::Entity::find()
        .filter(sys_resource::Column::Enabled.eq(true))
        .all(db)
        .await
        .map_err(|error| ApiErrors::Internal(anyhow::anyhow!(error)))?;

    let resource_ids: Vec<i64> = resources.iter().map(|resource| resource.id).collect();
    let actions_by_resource = load_actions_by_resource(db, &resource_ids).await?;

    let rules = resources
        .into_iter()
        .map(|resource| {
            let actions = match actions_by_resource.get(&resource.id) {
                Some(actions) => actions.clone(),
                None => Vec::new(),
            };

            ResourcePermissionRule::new(resource.method.as_str(), resource.path, actions)
        })
        .collect();

    Ok(ResourcePermissionPolicy::new(rules))
}

async fn load_actions_by_resource(
    db: &DbConn,
    resource_ids: &[i64],
) -> ApiResult<HashMap<i64, Vec<String>>> {
    if resource_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let bindings = sys_action_resource::Entity::find()
        .filter(sys_action_resource::Column::ResourceId.is_in(resource_ids.iter().copied()))
        .find_also_related(sys_menu::Entity)
        .all(db)
        .await
        .map_err(|error| ApiErrors::Internal(anyhow::anyhow!(error)))?;

    let mut actions_by_resource = HashMap::<i64, Vec<String>>::new();
    for (binding, menu) in bindings {
        let Some(menu) = menu else {
            continue;
        };
        if menu.menu_type != sys_menu::MenuType::Button
            || !menu.enabled
            || menu.auth_mark.is_empty()
        {
            continue;
        }

        actions_by_resource
            .entry(binding.resource_id)
            .or_default()
            .push(menu.auth_mark);
    }

    Ok(actions_by_resource)
}
