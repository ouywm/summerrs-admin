use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use common::response::PageResponse;
use model::dto::sys_role::{CreateRoleDto, RolePermissionDto, RoleQueryDto, UpdateRoleDto};
use model::entity::sys_role;
use model::entity::sys_role_menu;
use model::entity::sys_user_role;
use model::vo::sys_role::{RolePermissionVo, RoleVo};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use spring::plugin::Service;

use crate::plugin::sea_orm_plugin::DbConn;

#[derive(Clone, Service)]
pub struct SysRoleService {
    #[inject(component)]
    db: DbConn,
}

impl SysRoleService {
    /// 角色列表（分页+筛选）
    pub async fn list_roles(&self, query: RoleQueryDto) -> ApiResult<PageResponse<RoleVo>> {
        let mut select = sys_role::Entity::find();

        if let Some(ref name) = query.role_name {
            select = select.filter(sys_role::Column::RoleName.contains(name));
        }
        if let Some(ref code) = query.role_code {
            select = select.filter(sys_role::Column::RoleCode.contains(code));
        }
        if let Some(ref desc) = query.description {
            select = select.filter(sys_role::Column::Description.contains(desc));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(sys_role::Column::Enabled.eq(enabled));
        }
        if let Some(start_dt) = query.start_time {
            select = select.filter(sys_role::Column::CreateTime.gte(start_dt));
        }
        if let Some(end_dt) = query.end_time {
            select = select.filter(sys_role::Column::CreateTime.lte(end_dt));
        }

        let paginator = select.paginate(&self.db, query.page.size);
        let total = paginator.num_items().await.context("查询角色总数失败")?;
        let roles = paginator
            .fetch_page(query.page.page_index())
            .await
            .context("查询角色列表失败")?;

        let records: Vec<RoleVo> = roles.into_iter().map(RoleVo::from).collect();

        Ok(PageResponse::new(records, query.page.current, query.page.size, total))
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
        let all_menus = model::entity::sys_menu::Entity::find()
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

        Ok(())
    }
}
