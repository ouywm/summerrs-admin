use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use summer::plugin::Service;
use summer_auth::{LoginId, SessionManager};
use summer_common::error::{ApiErrors, ApiResult};
use summer_system_model::dto::sys_role::{
    CreateRoleDto, RolePermissionDto, RoleQueryDto, UpdateRoleDto,
};
use summer_system_model::entity::sys_role;
use summer_system_model::entity::sys_role_menu;
use summer_system_model::entity::sys_user_role;
use summer_system_model::vo::sys_role::{RolePermissionVo, RoleVo};

use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysRoleService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    auth: SessionManager,
}

impl SysRoleService {
    /// 角色列表（分页+筛选）
    pub async fn list_roles(
        &self,
        query: RoleQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RoleVo>> {
        let page = sys_role::Entity::find()
            .filter(query)
            .page(&self.db, &pagination)
            .await
            .context("查询角色列表失败")?;

        let page = page.map(RoleVo::from);
        Ok(page)
    }

    /// 创建角色
    pub async fn create_role(&self, dto: CreateRoleDto) -> ApiResult<()> {
        // 检查角色编码是否已存在
        let existing = sys_role::Entity::find()
            .filter(sys_role::Column::RoleCode.eq(&dto.role_code))
            .one(&self.db)
            .await
            .context("检查角色编码失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "角色编码已存在: {}",
                dto.role_code
            )));
        }

        let role: sys_role::ActiveModel = dto.into();
        role.insert(&self.db).await.context("创建角色失败")?;
        Ok(())
    }

    /// 更新角色
    pub async fn update_role(&self, id: i64, dto: UpdateRoleDto) -> ApiResult<()> {
        let role = sys_role::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询角色失败")?
            .ok_or_else(|| ApiErrors::NotFound("角色不存在".to_string()))?;

        let mut active: sys_role::ActiveModel = role.into();
        dto.apply_to(&mut active);
        active.update(&self.db).await.context("更新角色失败")?;
        Ok(())
    }

    /// 删除角色
    pub async fn delete_role(&self, id: i64) -> ApiResult<()> {
        // 检查是否有用户关联该角色
        let user_count = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::RoleId.eq(id))
            .count(&self.db)
            .await
            .context("查询角色用户关联失败")?;

        if user_count > 0 {
            return Err(ApiErrors::BadRequest(
                "该角色下存在用户，无法删除".to_string(),
            ));
        }

        // 删除角色菜单关联
        sys_role_menu::Entity::delete_many()
            .filter(sys_role_menu::Column::RoleId.eq(id))
            .exec(&self.db)
            .await
            .context("删除角色菜单关联失败")?;

        let result = sys_role::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除角色失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("角色不存在".to_string()));
        }

        Ok(())
    }

    /// 获取角色的菜单权限
    pub async fn get_role_permissions(&self, role_id: i64) -> ApiResult<RolePermissionVo> {
        // 确认角色存在
        sys_role::Entity::find_by_id(role_id)
            .one(&self.db)
            .await
            .context("查询角色失败")?
            .ok_or_else(|| ApiErrors::NotFound("角色不存在".to_string()))?;

        // 查询已分配的菜单 ID
        let role_menus = sys_role_menu::Entity::find()
            .filter(sys_role_menu::Column::RoleId.eq(role_id))
            .all(&self.db)
            .await
            .context("查询角色菜单权限失败")?;

        let all_menu_ids: Vec<i64> = role_menus.iter().map(|rm| rm.menu_id).collect();

        // 查询所有菜单，找出叶子节点（checked）和中间节点（half-checked）
        let all_menus = summer_system_model::entity::sys_menu::Entity::find()
            .all(&self.db)
            .await
            .context("查询菜单失败")?;

        let parent_ids: std::collections::HashSet<i64> = all_menus
            .iter()
            .filter(|m| all_menu_ids.contains(&m.id))
            .filter(|m| {
                // 如果有子菜单也在 all_menu_ids 中，则该节点是 half-checked
                all_menus
                    .iter()
                    .any(|child| child.parent_id == m.id && all_menu_ids.contains(&child.id))
            })
            .map(|m| m.id)
            .collect();

        let checked_keys: Vec<i64> = all_menu_ids
            .iter()
            .filter(|id| !parent_ids.contains(id))
            .copied()
            .collect();

        let half_checked_keys: Vec<i64> = parent_ids.into_iter().collect();

        Ok(RolePermissionVo {
            checked_keys,
            half_checked_keys,
        })
    }

    /// 保存角色的菜单权限
    pub async fn save_role_permissions(
        &self,
        role_id: i64,
        dto: RolePermissionDto,
    ) -> ApiResult<()> {
        // 确认角色存在
        sys_role::Entity::find_by_id(role_id)
            .one(&self.db)
            .await
            .context("查询角色失败")?
            .ok_or_else(|| ApiErrors::NotFound("角色不存在".to_string()))?;

        // 删除旧的关联
        sys_role_menu::Entity::delete_many()
            .filter(sys_role_menu::Column::RoleId.eq(role_id))
            .exec(&self.db)
            .await
            .context("删除旧的角色菜单关联失败")?;

        // 批量插入新关联
        if !dto.menu_ids.is_empty() {
            let models: Vec<sys_role_menu::ActiveModel> = dto
                .menu_ids
                .into_iter()
                .map(|menu_id| sys_role_menu::ActiveModel {
                    role_id: Set(role_id),
                    menu_id: Set(menu_id),
                    ..Default::default()
                })
                .collect();

            sys_role_menu::Entity::insert_many(models)
                .exec(&self.db)
                .await
                .context("保存角色菜单权限失败")?;
        }

        // 查询该角色下所有用户，强制刷新 token 以获取最新权限
        let user_ids: Vec<i64> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::RoleId.eq(role_id))
            .all(&self.db)
            .await
            .context("查询角色用户失败")?
            .into_iter()
            .map(|ur| ur.user_id)
            .collect();

        for user_id in user_ids {
            let _ = self.auth.force_refresh(&LoginId::new(user_id)).await;
        }

        Ok(())
    }
}
