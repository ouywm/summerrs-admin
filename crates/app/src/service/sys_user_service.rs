use anyhow::Context;
use common::crypto::{hash_password, verify_password};
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_user::{CreateUserDto, ResetPasswordDto, UpdateUserDto, UserQueryDto};
use model::dto::user_profile::{ChangePasswordDto, UpdateProfileDto};
use model::entity::sys_menu;
use model::entity::sys_role;
use model::entity::sys_role_menu;
use model::entity::sys_user;
use model::entity::sys_user_role;
use model::vo::sys_role::RoleDetailVo;
use model::vo::sys_user::{UserDetailVo, UserInfoVo, UserVo};
use model::vo::user_profile::UserProfileVo;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect,
    RelationTrait, Set, TransactionTrait,
};
use spring::plugin::Service;

use spring_sa_token::StpUtil;

use crate::plugin::pagination::{Page, Pagination, PaginationExt};
use crate::plugin::sea_orm_plugin::DbConn;

#[derive(Clone, Service)]
pub struct SysUserService {
    #[inject(component)]
    db: DbConn,
}

impl SysUserService {
    /// 从 JWT payload 获取操作人昵称
    async fn get_operator_name(&self, login_id: &str) -> ApiResult<String> {
        let token = StpUtil::get_token_by_login_id(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let extra = StpUtil::get_extra_data(&token)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let name = extra
            .and_then(|v| {
                v.get("nick_name")
                    .and_then(|n| n.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("无法获取操作人昵称")))?;
        Ok(name)
    }

    /// 获取当前登录用户信息（含角色和按钮权限）
    pub async fn get_user_info(&self, login_id: &str) -> ApiResult<UserInfoVo> {
        let user_id: i64 = login_id
            .parse()
            .map_err(|_| ApiErrors::BadRequest("无效的用户ID".to_string()))?;

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
    pub async fn list_users(&self, query: UserQueryDto, pagination: Pagination) -> ApiResult<Page<UserVo>> {
        let mut select = sys_user::Entity::find();

        if let Some(ref name) = query.user_name {
            select = select.filter(sys_user::Column::UserName.contains(name));
        }
        if let Some(ref phone) = query.user_phone {
            select = select.filter(sys_user::Column::Phone.contains(phone));
        }
        if let Some(ref email) = query.user_email {
            select = select.filter(sys_user::Column::Email.contains(email));
        }
        if let Some(status) = query.status {
            select = select.filter(sys_user::Column::Status.eq(status));
        }
        if let Some(gender) = query.user_gender {
            select = select.filter(sys_user::Column::Gender.eq(gender));
        }

        let page = select
            .page(&self.db, &pagination)
            .await
            .context("查询用户列表失败")?;

        let page = page.map(UserVo::from_model);
        Ok(page)
    }

    /// 创建用户
    pub async fn create_user(&self, dto: CreateUserDto, login_id: &str) -> ApiResult<()> {
        let role_ids = dto.role_ids.clone();
        let operator = self.get_operator_name(login_id).await?;

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
                        .map_err(|e| ApiErrors::Internal(e))?;

                    if existing.is_some() {
                        return Err(ApiErrors::Conflict(format!(
                            "用户名已存在: {}",
                            dto.user_name
                        )));
                    }

                    // 创建用户
                    let hashed = hash_password(common::crypto::DEFAULT_PASSWORD)
                        .context("密码加密失败")
                        .map_err(|e| ApiErrors::Internal(e))?;
                    let user_model = dto.into_active_model(hashed, operator);
                    let user = user_model
                        .insert(txn)
                        .await
                        .context("创建用户失败")
                        .map_err(|e| ApiErrors::Internal(e))?;

                    // 分配角色
                    if let Some(role_ids) = role_ids {
                        if !role_ids.is_empty() {
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
                                .map_err(|e| ApiErrors::Internal(e))?;
                        }
                    }

                    Ok(())
                })
            })
            .await
            .map_err(|e| match e {
                sea_orm::TransactionError::Connection(err) => {
                    ApiErrors::Internal(anyhow::anyhow!("数据库连接错误: {}", err))
                }
                sea_orm::TransactionError::Transaction(err) => err,
            })
    }

    /// 更新用户
    pub async fn update_user(&self, id: i64, dto: UpdateUserDto, login_id: &str) -> ApiResult<()> {
        let role_ids = dto.role_ids.clone();
        let operator = self.get_operator_name(login_id).await?;

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    // 查询用户
                    let user = sys_user::Entity::find_by_id(id)
                        .one(txn)
                        .await
                        .context("查询用户失败")
                        .map_err(|e| ApiErrors::Internal(e))?
                        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

                    // 更新用户信息
                    let mut active: sys_user::ActiveModel = user.into();
                    dto.apply_to(&mut active, &operator);
                    active
                        .update(txn)
                        .await
                        .context("更新用户失败")
                        .map_err(|e| ApiErrors::Internal(e))?;

                    // 更新角色
                    if let Some(role_ids) = role_ids {
                        // 删除现有角色关联
                        sys_user_role::Entity::delete_many()
                            .filter(sys_user_role::Column::UserId.eq(id))
                            .exec(txn)
                            .await
                            .context("删除用户角色关联失败")
                            .map_err(|e| ApiErrors::Internal(e))?;

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
                                .map_err(|e| ApiErrors::Internal(e))?;
                        }
                    }

                    Ok(())
                })
            })
            .await
            .map_err(|e| match e {
                sea_orm::TransactionError::Connection(err) => {
                    ApiErrors::Internal(anyhow::anyhow!("数据库连接错误: {}", err))
                }
                sea_orm::TransactionError::Transaction(err) => err,
            })
    }

    /// 删除用户（逻辑删除，设置状态为注销）
    pub async fn delete_user(&self, id: i64) -> ApiResult<()> {
        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                Box::pin(async move {
                    // 查询用户是否存在
                    let user = sys_user::Entity::find_by_id(id)
                        .one(txn)
                        .await
                        .context("查询用户失败")
                        .map_err(|e| ApiErrors::Internal(e))?
                        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

                    // 检查是否已经注销
                    if user.status == sys_user::UserStatus::Cancelled {
                        return Err(ApiErrors::BadRequest("用户已注销".to_string()));
                    }

                    // 设置状态为注销
                    let mut active: sys_user::ActiveModel = user.into();
                    active.status = Set(sys_user::UserStatus::Cancelled);
                    active
                        .update(txn)
                        .await
                        .context("注销用户失败")
                        .map_err(|e| ApiErrors::Internal(e))?;

                    Ok(())
                })
            })
            .await
            .map_err(|e| match e {
                sea_orm::TransactionError::Connection(err) => {
                    ApiErrors::Internal(anyhow::anyhow!("数据库连接错误: {}", err))
                }
                sea_orm::TransactionError::Transaction(err) => err,
            })
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
        let hashed_password = hash_password(&dto.new_password)
            .context("密码加密失败")?;

        // 更新密码
        let mut active: sys_user::ActiveModel = user.into();
        active.password = Set(hashed_password);
        active
            .update(&self.db)
            .await
            .context("更新密码失败")?;

        Ok(())
    }

    /// 修改个人密码
    pub async fn change_password(&self, login_id: &str, dto: ChangePasswordDto) -> ApiResult<()> {
        let user_id: i64 = login_id
            .parse()
            .map_err(|_| ApiErrors::BadRequest("无效的用户ID".to_string()))?;

        // 查询用户
        let user = sys_user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 验证旧密码
        let is_valid = verify_password(&dto.old_password, &user.password)
            .context("密码验证失败")?;
        if !is_valid {
            return Err(ApiErrors::BadRequest("当前密码不正确".to_string()));
        }

        // 加密新密码
        let hashed_password = hash_password(&dto.new_password)
            .context("密码加密失败")?;

        // 更新密码
        let mut active: sys_user::ActiveModel = user.into();
        active.password = Set(hashed_password);
        active
            .update(&self.db)
            .await
            .context("更新密码失败")?;

        Ok(())
    }

    /// 更新个人信息
    pub async fn update_profile(&self, login_id: &str, dto: UpdateProfileDto) -> ApiResult<UserProfileVo> {
        let user_id: i64 = login_id
            .parse()
            .map_err(|_| ApiErrors::BadRequest("无效的用户ID".to_string()))?;

        // 查询用户
        let user = sys_user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;

        // 检查邮箱是否被其他用户使用
        if let Some(ref email) = dto.email {
            if !email.is_empty() {
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
        }

        // 更新用户信息
        let mut active: sys_user::ActiveModel = user.into();
        dto.apply_to(&mut active);

        let updated_user = active
            .update(&self.db)
            .await
            .context("更新个人信息失败")?;

        Ok(UserProfileVo::from_model(updated_user))
    }
}
