use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use summer::plugin::Service;
use summer_auth::ResourcePermissionRegistry;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_system_model::dto::sys_resource::{
    CreateResourceDto, ResourceQueryDto, SaveActionResourcesDto, UpdateResourceDto,
    UpdateResourceEnabledDto,
};
use summer_system_model::entity::{sys_action_resource, sys_menu, sys_resource};
use summer_system_model::vo::sys_resource::{
    ActionOptionVo, ActionResourceVo, ResourceActionVo, ResourceOptionVo, ResourceVo,
};

use crate::resource_permission::load_resource_permission_policy;

#[derive(Clone, Service)]
pub struct SysResourceService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    registry: ResourcePermissionRegistry,
}

impl SysResourceService {
    pub async fn list(
        &self,
        query: ResourceQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ResourceVo>> {
        let page = sys_resource::Entity::find()
            .filter(query)
            .order_by_asc(sys_resource::Column::Method)
            .order_by_asc(sys_resource::Column::Path)
            .page(&self.db, &pagination)
            .await
            .context("查询后端 API 资源列表失败")?;

        Ok(page.map(ResourceVo::from))
    }

    pub async fn options(&self) -> ApiResult<Vec<ResourceOptionVo>> {
        let resources = sys_resource::Entity::find()
            .order_by_asc(sys_resource::Column::Method)
            .order_by_asc(sys_resource::Column::Path)
            .all(&self.db)
            .await
            .context("查询后端 API 资源选项失败")?;

        Ok(resources.into_iter().map(ResourceOptionVo::from).collect())
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<ResourceVo> {
        Ok(ResourceVo::from(self.find_resource_by_id(id).await?))
    }

    pub async fn create(&self, dto: CreateResourceDto) -> ApiResult<()> {
        self.ensure_resource_code_unique(&dto.resource_code, None)
            .await?;
        self.ensure_method_path_unique(dto.method, &dto.path, None)
            .await?;

        let active: sys_resource::ActiveModel = dto.into();
        active
            .insert(&self.db)
            .await
            .context("创建后端 API 资源失败")?;
        self.reload_policy().await?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateResourceDto) -> ApiResult<()> {
        let current = self.find_resource_by_id(id).await?;
        if let Some(resource_code) = dto.resource_code.as_deref() {
            self.ensure_resource_code_unique(resource_code, Some(id))
                .await?;
        }

        let method = dto.method.unwrap_or(current.method);
        let path = dto.path.as_deref().unwrap_or(&current.path);
        self.ensure_method_path_unique(method, path, Some(id))
            .await?;

        let mut active = current.into_active_model();
        dto.apply_to(&mut active);
        active
            .update(&self.db)
            .await
            .context("更新后端 API 资源失败")?;
        self.reload_policy().await?;
        Ok(())
    }

    pub async fn update_enabled(&self, id: i64, dto: UpdateResourceEnabledDto) -> ApiResult<()> {
        let mut active = self.find_resource_by_id(id).await?.into_active_model();
        active.enabled = Set(dto.enabled);
        active
            .update(&self.db)
            .await
            .context("更新后端 API 资源状态失败")?;
        self.reload_policy().await?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        self.find_resource_by_id(id).await?;

        let binding_count = sys_action_resource::Entity::find()
            .filter(sys_action_resource::Column::ResourceId.eq(id))
            .count(&self.db)
            .await
            .context("查询资源绑定数量失败")?;
        if binding_count > 0 {
            return Err(ApiErrors::BadRequest(
                "资源已绑定按钮权限，无法删除".to_string(),
            ));
        }

        let result = sys_resource::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除后端 API 资源失败")?;
        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("后端 API 资源不存在".to_string()));
        }
        self.reload_policy().await?;
        Ok(())
    }

    pub async fn get_action_resources(&self, action_menu_id: i64) -> ApiResult<ActionResourceVo> {
        let action = self.find_action_by_id(action_menu_id).await?;
        let bindings = sys_action_resource::Entity::find()
            .filter(sys_action_resource::Column::ActionMenuId.eq(action_menu_id))
            .find_also_related(sys_resource::Entity)
            .all(&self.db)
            .await
            .context("查询按钮资源绑定失败")?;

        let resources = bindings
            .into_iter()
            .filter_map(|(_, resource)| resource.map(ResourceOptionVo::from))
            .collect();

        Ok(ActionResourceVo::from_action(action, resources))
    }

    pub async fn save_action_resources(
        &self,
        action_menu_id: i64,
        dto: SaveActionResourcesDto,
    ) -> ApiResult<()> {
        self.find_action_by_id(action_menu_id).await?;
        let resource_ids = dedupe_ids(dto.resource_ids);
        self.ensure_resources_exist(&resource_ids).await?;

        let txn = self.db.begin().await.context("开启按钮资源绑定事务失败")?;

        sys_action_resource::Entity::delete_many()
            .filter(sys_action_resource::Column::ActionMenuId.eq(action_menu_id))
            .exec(&txn)
            .await
            .context("删除旧的按钮资源绑定失败")?;

        if !resource_ids.is_empty() {
            let bindings =
                resource_ids
                    .into_iter()
                    .map(|resource_id| sys_action_resource::ActiveModel {
                        id: Default::default(),
                        action_menu_id: Set(action_menu_id),
                        resource_id: Set(resource_id),
                    });
            sys_action_resource::Entity::insert_many(bindings)
                .exec(&txn)
                .await
                .context("保存按钮资源绑定失败")?;
        }

        txn.commit().await.context("提交按钮资源绑定事务失败")?;
        self.reload_policy().await?;
        Ok(())
    }

    pub async fn get_resource_actions(&self, resource_id: i64) -> ApiResult<ResourceActionVo> {
        let resource = self.find_resource_by_id(resource_id).await?;
        let bindings = sys_action_resource::Entity::find()
            .filter(sys_action_resource::Column::ResourceId.eq(resource_id))
            .find_also_related(sys_menu::Entity)
            .all(&self.db)
            .await
            .context("查询资源关联按钮失败")?;

        let actions = bindings
            .into_iter()
            .filter_map(|(_, action)| action.map(ActionOptionVo::from))
            .collect();

        Ok(ResourceActionVo::from_resource(resource, actions))
    }

    pub async fn reload_policy(&self) -> ApiResult<()> {
        let policy = load_resource_permission_policy(&self.db).await?;
        self.registry.replace(policy);
        Ok(())
    }

    async fn find_resource_by_id(&self, id: i64) -> ApiResult<sys_resource::Model> {
        sys_resource::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询后端 API 资源详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("后端 API 资源不存在".to_string()))
    }

    async fn find_action_by_id(&self, action_menu_id: i64) -> ApiResult<sys_menu::Model> {
        let action = sys_menu::Entity::find_by_id(action_menu_id)
            .one(&self.db)
            .await
            .context("查询按钮权限失败")?
            .ok_or_else(|| ApiErrors::NotFound("按钮权限不存在".to_string()))?;

        if action.menu_type != sys_menu::MenuType::Button {
            return Err(ApiErrors::BadRequest(
                "action_menu_id 必须指向按钮权限菜单".to_string(),
            ));
        }
        if action.auth_mark.is_empty() {
            return Err(ApiErrors::BadRequest("按钮权限标识不能为空".to_string()));
        }

        Ok(action)
    }

    async fn ensure_resources_exist(&self, resource_ids: &[i64]) -> ApiResult<()> {
        if resource_ids.is_empty() {
            return Ok(());
        }

        let count = sys_resource::Entity::find()
            .filter(sys_resource::Column::Id.is_in(resource_ids.iter().copied()))
            .count(&self.db)
            .await
            .context("校验后端 API 资源失败")?;

        if count != resource_ids.len() as u64 {
            return Err(ApiErrors::BadRequest(
                "存在无效的后端 API 资源ID".to_string(),
            ));
        }
        Ok(())
    }

    async fn ensure_resource_code_unique(
        &self,
        resource_code: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = sys_resource::Entity::find()
            .filter(sys_resource::Column::ResourceCode.eq(resource_code));
        if let Some(exclude_id) = exclude_id {
            query = query.filter(sys_resource::Column::Id.ne(exclude_id));
        }

        let existing = query
            .one(&self.db)
            .await
            .context("检查资源编码是否重复失败")?;
        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "资源编码已存在: {}",
                resource_code
            )));
        }
        Ok(())
    }

    async fn ensure_method_path_unique(
        &self,
        method: sys_resource::ResourceMethod,
        path: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = sys_resource::Entity::find()
            .filter(sys_resource::Column::Method.eq(method))
            .filter(sys_resource::Column::Path.eq(path));
        if let Some(exclude_id) = exclude_id {
            query = query.filter(sys_resource::Column::Id.ne(exclude_id));
        }

        let existing = query
            .one(&self.db)
            .await
            .context("检查资源请求方法和路径是否重复失败")?;
        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "资源请求方法和路径已存在: {} {}",
                method.as_str(),
                path
            )));
        }
        Ok(())
    }
}

fn dedupe_ids(ids: Vec<i64>) -> Vec<i64> {
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        if id > 0 && !result.contains(&id) {
            result.push(id);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::dedupe_ids;

    #[test]
    fn dedupe_ids_removes_duplicates_and_invalid_values() {
        assert_eq!(dedupe_ids(vec![3, 0, 1, 3, -1, 2, 1]), vec![3, 1, 2]);
    }
}
