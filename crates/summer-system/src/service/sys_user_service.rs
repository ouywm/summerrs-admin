use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
    Set, TransactionTrait,
};
use summer::plugin::Service;
use summer_auth::{LoginId, SessionManager};
use summer_common::crypto::{hash_password, verify_password};
use summer_common::error::{ApiErrors, ApiResult};
use summer_system_model::dto::sys_user::{
    CreateUserDto, ResetPasswordDto, UpdateUserDto, UserQueryDto,
};
use summer_system_model::dto::user_profile::{ChangePasswordDto, UpdateProfileDto};
use summer_system_model::entity::sys_file;
use summer_system_model::entity::sys_menu;
use summer_system_model::entity::sys_notice_target;
use summer_system_model::entity::sys_notice_user;
use summer_system_model::entity::sys_role;
use summer_system_model::entity::sys_role_menu;
use summer_system_model::entity::sys_user;
use summer_system_model::entity::sys_user::UserStatus;
use summer_system_model::entity::sys_user_role;
use summer_system_model::vo::sys_role::RoleDetailVo;
use summer_system_model::vo::sys_user::{UserDetailVo, UserInfoVo, UserVo};
use summer_system_model::vo::user_profile::UserProfileVo;

use crate::socketio::service::{KickoutPayload, SocketGatewayService};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysUserService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    auth: SessionManager,
    #[inject(component)]
    socket_gateway: SocketGatewayService,
}

impl SysUserService {
    /// 获取当前登录用户信息（含角色和按钮权限）
    pub async fn get_user_info(&self, login_id: &LoginId) -> ApiResult<UserInfoVo> {
        let user_id = login_id.user_id;

        let user = sys_user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 查询角色编码
        let roles: Vec<String> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(user.id))
            .find_also_related(sys_role::Entity)
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .filter_map(|(_, role)| role.map(|r| r.role_code))
            .collect();

        // 查询按钮权限标识
        let role_ids: Vec<i64> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(user.id))
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .map(|ur| ur.role_id)
            .collect();

        let buttons: Vec<String> = if role_ids.is_empty() {
            vec![]
        } else {
            sys_menu::Entity::find()
                .join(JoinType::InnerJoin, sys_menu::Relation::SysRoleMenu.def())
                .filter(sys_role_menu::Column::RoleId.is_in(role_ids))
                .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Button))
                .filter(sys_menu::Column::Enabled.eq(true))
                .all(&self.db)
                .await
                .context("查询按钮权限失败")?
                .into_iter()
                .filter(|m| !m.auth_mark.is_empty())
                .map(|m| m.auth_mark)
                .collect()
        };

        Ok(UserInfoVo {
            user_id: user.id,
            user_name: user.user_name,
            email: user.email,
            avatar: user.avatar,
            roles,
            buttons,
        })
    }

    /// 获取用户详情（根据用户 ID）
    pub async fn get_user_detail(&self, id: i64) -> ApiResult<UserDetailVo> {
        let user = sys_user::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 查询角色详细信息
        let role_details: Vec<RoleDetailVo> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(id))
            .find_also_related(sys_role::Entity)
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .filter_map(|(_, role)| {
                role.map(|r| RoleDetailVo {
                    role_id: r.id,
                    role_name: r.role_name,
                    role_code: r.role_code,
                })
            })
            .collect();

        Ok(UserDetailVo {
            user: UserVo::from_model(user),
            roles: role_details,
        })
    }

    /// 用户列表（分页+筛选）
    pub async fn list_users(
        &self,
        query: UserQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<UserVo>> {
        let page = sys_user::Entity::find()
            .filter(query)
            .page(&self.db, &pagination)
            .await
            .context("查询用户列表失败")?;

        let page = page.map(UserVo::from_model);
        Ok(page)
    }

    /// 创建用户
    pub async fn create_user(&self, dto: CreateUserDto, operator: &str) -> ApiResult<()> {
        let role_ids = dto.role_ids.clone();
        let operator = operator.to_string();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    // 检查用户名是否存在
                    let existing = sys_user::Entity::find()
                        .filter(sys_user::Column::UserName.eq(&dto.user_name))
                        .one(txn)
                        .await
                        .context("检查用户名失败")
                        .map_err(ApiErrors::Internal)?;

                    if existing.is_some() {
                        return Err(ApiErrors::Conflict(format!(
                            "用户名已存在: {}",
                            dto.user_name
                        )));
                    }

                    // 创建用户
                    let hashed = hash_password(summer_common::crypto::DEFAULT_PASSWORD)
                        .context("密码加密失败")
                        .map_err(ApiErrors::Internal)?;
                    let user_model = dto.into_active_model(hashed, operator);
                    let user = user_model
                        .insert(txn)
                        .await
                        .context("创建用户失败")
                        .map_err(ApiErrors::Internal)?;

                    // 分配角色
                    if let Some(role_ids) = role_ids
                        && !role_ids.is_empty()
                    {
                        let models: Vec<sys_user_role::ActiveModel> = role_ids
                            .into_iter()
                            .map(|role_id| sys_user_role::ActiveModel {
                                user_id: Set(user.id),
                                role_id: Set(role_id),
                                ..Default::default()
                            })
                            .collect();

                        sys_user_role::Entity::insert_many(models)
                            .exec(txn)
                            .await
                            .context("分配角色失败")
                            .map_err(ApiErrors::Internal)?;
                    }

                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    /// 更新用户
    pub async fn update_user(&self, id: i64, dto: UpdateUserDto, operator: &str) -> ApiResult<()> {
        let role_ids = dto.role_ids.clone();
        let has_role_change = role_ids.is_some();
        let operator = operator.to_string();
        let login_id = LoginId::admin(id);

        let (previous_status, current_status) = self
            .db
            .transaction::<_, (UserStatus, UserStatus), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    // 查询用户
                    let user = sys_user::Entity::find_by_id(id)
                        .one(txn)
                        .await
                        .context("查询用户失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;
                    let previous_status = user.status;
                    let current_status = dto.status.unwrap_or(previous_status);

                    // 更新用户信息
                    let mut active: sys_user::ActiveModel = user.into();
                    dto.apply_to(&mut active, &operator);
                    active
                        .update(txn)
                        .await
                        .context("更新用户失败")
                        .map_err(ApiErrors::Internal)?;

                    // 更新角色
                    if let Some(role_ids) = role_ids {
                        // 删除现有角色关联
                        sys_user_role::Entity::delete_many()
                            .filter(sys_user_role::Column::UserId.eq(id))
                            .exec(txn)
                            .await
                            .context("删除用户角色关联失败")
                            .map_err(ApiErrors::Internal)?;

                        // 批量插入新角色
                        if !role_ids.is_empty() {
                            let models: Vec<sys_user_role::ActiveModel> = role_ids
                                .into_iter()
                                .map(|role_id| sys_user_role::ActiveModel {
                                    user_id: Set(id),
                                    role_id: Set(role_id),
                                    ..Default::default()
                                })
                                .collect();

                            sys_user_role::Entity::insert_many(models)
                                .exec(txn)
                                .await
                                .context("分配角色失败")
                                .map_err(ApiErrors::Internal)?;
                        }
                    }

                    Ok((previous_status, current_status))
                })
            })
            .await?;

        if previous_status != UserStatus::Disabled && current_status == UserStatus::Disabled {
            self.disable_user_runtime(&login_id).await?;
        } else if previous_status == UserStatus::Disabled && current_status == UserStatus::Enabled {
            self.enable_user_runtime(&login_id).await?;
        }

        // 角色变更后，强制用户刷新 token 以获取最新权限
        if has_role_change && current_status != UserStatus::Disabled {
            let _ = self.auth.force_refresh(&login_id).await;
        }

        Ok(())
    }

    /// 删除用户（物理删除，并清理关联资源）
    pub async fn delete_user(&self, id: i64) -> ApiResult<()> {
        let login_id = LoginId::admin(id);

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                Box::pin(async move {
                    let user = sys_user::Entity::find_by_id(id)
                        .one(txn)
                        .await
                        .context("查询用户失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

                    if user.status != sys_user::UserStatus::Disabled {
                        return Err(ApiErrors::BadRequest(
                            "该用户仍处于启用状态，请先禁用后再删除".to_string(),
                        ));
                    }

                    sys_user_role::Entity::delete_many()
                        .filter(sys_user_role::Column::UserId.eq(id))
                        .exec(txn)
                        .await
                        .context("删除用户角色关联失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_notice_user::Entity::delete_many()
                        .filter(sys_notice_user::Column::UserId.eq(id))
                        .exec(txn)
                        .await
                        .context("删除用户公告接收记录失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_notice_target::Entity::delete_many()
                        .filter(
                            sys_notice_target::Column::TargetType
                                .eq(sys_notice_target::NoticeTargetType::User),
                        )
                        .filter(sys_notice_target::Column::TargetId.eq(id))
                        .exec(txn)
                        .await
                        .context("删除用户公告目标关联失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_file::Entity::update_many()
                        .set(sys_file::ActiveModel {
                            upload_by_id: Set(None),
                            ..Default::default()
                        })
                        .filter(sys_file::Column::UploadById.eq(id))
                        .exec(txn)
                        .await
                        .context("清理用户文件归属失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_user::Entity::delete_by_id(id)
                        .exec(txn)
                        .await
                        .context("删除用户失败")
                        .map_err(ApiErrors::Internal)?;

                    Ok(())
                })
            })
            .await?;

        self.revoke_user_sessions(&login_id).await?;

        Ok(())
    }

    /// 重置用户密码
    pub async fn reset_password(&self, id: i64, dto: ResetPasswordDto) -> ApiResult<()> {
        // 查询用户是否存在
        let user = sys_user::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 加密新密码
        let hashed_password = hash_password(&dto.new_password).context("密码加密失败")?;

        // 更新密码
        let mut active: sys_user::ActiveModel = user.into();
        active.password = Set(hashed_password);
        active.update(&self.db).await.context("更新密码失败")?;

        Ok(())
    }

    /// 修改个人密码
    pub async fn change_password(
        &self,
        login_id: &LoginId,
        dto: ChangePasswordDto,
    ) -> ApiResult<()> {
        let user_id = login_id.user_id;

        // 查询用户
        let user = sys_user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 验证旧密码
        let is_valid =
            verify_password(&dto.old_password, &user.password).context("密码验证失败")?;
        if !is_valid {
            return Err(ApiErrors::BadRequest("当前密码不正确".to_string()));
        }

        // 加密新密码
        let hashed_password = hash_password(&dto.new_password).context("密码加密失败")?;

        // 更新密码
        let mut active: sys_user::ActiveModel = user.into();
        active.password = Set(hashed_password);
        active.update(&self.db).await.context("更新密码失败")?;

        Ok(())
    }

    /// 更新个人信息
    pub async fn update_profile(
        &self,
        login_id: &LoginId,
        dto: UpdateProfileDto,
    ) -> ApiResult<UserProfileVo> {
        let user_id = login_id.user_id;

        // 查询用户
        let user = sys_user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 检查邮箱是否被其他用户使用
        if let Some(ref email) = dto.email
            && !email.is_empty()
        {
            let existing = sys_user::Entity::find()
                .filter(sys_user::Column::Email.eq(email))
                .filter(sys_user::Column::Id.ne(user_id))
                .one(&self.db)
                .await
                .context("检查邮箱失败")?;

            if existing.is_some() {
                return Err(ApiErrors::Conflict("该邮箱已被其他用户使用".to_string()));
            }
        }

        // 更新用户信息
        let mut active: sys_user::ActiveModel = user.into();
        dto.apply_to(&mut active);

        let updated_user = active.update(&self.db).await.context("更新个人信息失败")?;

        Ok(UserProfileVo::from_model(updated_user))
    }

    async fn disable_user_runtime(&self, login_id: &LoginId) -> ApiResult<()> {
        self.auth
            .ban_user(login_id)
            .await
            .map_err(map_auth_runtime_error)?;
        self.revoke_user_sessions(login_id).await
    }

    async fn enable_user_runtime(&self, login_id: &LoginId) -> ApiResult<()> {
        self.auth
            .unban_user(login_id)
            .await
            .map_err(map_auth_runtime_error)
    }

    async fn revoke_user_sessions(&self, login_id: &LoginId) -> ApiResult<()> {
        self.auth
            .logout_all(login_id)
            .await
            .map_err(map_auth_runtime_error)?;
        self.socket_gateway
            .notify_and_disconnect(login_id, &KickoutPayload::account_disabled())
            .await?;
        Ok(())
    }
}

fn map_auth_runtime_error(err: impl std::fmt::Display) -> ApiErrors {
    ApiErrors::Internal(anyhow::anyhow!("{err}"))
}
