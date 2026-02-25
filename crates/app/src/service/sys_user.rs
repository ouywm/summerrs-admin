use anyhow::Context;
use sea_orm::*;
use spring::plugin::service::Service;
use spring_sea_orm::DbConn;

use model::dto::sys_user::CreateUserDto;
use model::entity::sys_user::{self, Entity as SysUser};

#[derive(Clone, Service)]
pub struct SysUserService {
    #[inject(component)]
    db: DbConn,
}

impl SysUserService {
    /// 创建用户（含业务校验）
    pub async fn create_user(&self, dto: CreateUserDto) -> anyhow::Result<sys_user::Model> {
        // 检查用户名是否重复
        let existing = SysUser::find()
            .filter(sys_user::Column::Username.eq(&dto.username))
            .one(&self.db)
            .await
            .context("检查用户名失败")?;

        if existing.is_some() {
            return Err(anyhow::anyhow!("用户名已存在: {}", dto.username));
        }

        // 检查邮箱是否重复
        if let Some(ref email) = dto.email {
            let existing = SysUser::find()
                .filter(sys_user::Column::Email.eq(email))
                .one(&self.db)
                .await
                .context("检查邮箱失败")?;

            if existing.is_some() {
                return Err(anyhow::anyhow!("邮箱已被注册: {}", email));
            }
        }

        // TODO: 密码加密，这里暂时明文存储
        let password = dto.password;

        let user = sys_user::ActiveModel {
            username: Set(dto.username),
            password: Set(password),
            nickname: Set(dto.nickname.unwrap_or_default()),
            email: Set(dto.email),
            phone: Set(dto.phone),
            status: Set(1),
            ..Default::default()
        };

        let user = user.insert(&self.db).await.context("创建用户失败")?;
        Ok(user)
    }

    /// 重置密码
    pub async fn reset_password(
        &self,
        user_id: i64,
        new_password: String,
    ) -> anyhow::Result<()> {
        let user = SysUser::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| anyhow::anyhow!("用户不存在"))?;

        // TODO: 密码加密
        let password = new_password;

        let mut user_active: sys_user::ActiveModel = user.into();
        user_active.password = Set(password);
        user_active.update(&self.db).await.context("重置密码失败")?;

        Ok(())
    }
}
