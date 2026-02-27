use anyhow::Context;
use common::crypto::verify_password;
use common::error::{ApiErrors, ApiResult};
use model::dto::auth::LoginDto;
use model::entity::sys_menu;
use model::entity::sys_role;
use model::entity::sys_user;
use model::entity::sys_user_role;
use model::vo::auth::LoginVo;
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait};
use spring::plugin::Service;
use spring_sa_token::StpUtil;

use crate::plugin::sea_orm_plugin::DbConn;

#[derive(Clone, Service)]
pub struct AuthService {
    #[inject(component)]
    db: DbConn,
}

impl AuthService {
    pub async fn login(&self, dto: LoginDto) -> ApiResult<LoginVo> {
        // 根据用户名查询用户
        let user = sys_user::Entity::find()
            .filter(sys_user::Column::UserName.eq(&dto.user_name))
            .one(&self.db)
            .await
            .context("查询用户失败")?
            .ok_or_else(|| ApiErrors::Unauthorized("用户名或密码错误".to_string()))?;

        // 验证用户状态
        if user.status == sys_user::UserStatus::Cancelled {
            return Err(ApiErrors::Forbidden("账号已注销".to_string()));
        }
        if user.status == sys_user::UserStatus::Abnormal {
            return Err(ApiErrors::Forbidden("账号状态异常".to_string()));
        }

        // 验证密码
        let valid = verify_password(&dto.password, &user.password)
            .map_err(|_| ApiErrors::Unauthorized("用户名或密码错误".to_string()))?;
        if !valid {
            return Err(ApiErrors::Unauthorized("用户名或密码错误".to_string()));
        }

        // 登录并获取 token（将用户名和昵称嵌入 JWT payload）
        let login_id = user.id.to_string();
        let token = StpUtil::login_with_extra(
            &login_id,
            serde_json::json!({
                "user_name": &user.user_name,
                "nick_name": &user.nick_name
            }),
        )
        .await
        .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;

        // 查询用户角色
        let roles: Vec<String> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(user.id))
            .find_also_related(sys_role::Entity)
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .filter_map(|(_, role)| role.map(|r| r.role_code))
            .collect();

        // 查询用户权限（按钮权限标识）
        let permissions: Vec<String> = self.get_user_permissions(user.id).await?;

        // 设置角色和权限到 sa-token
        StpUtil::set_roles(&login_id, roles)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        StpUtil::set_permissions(&login_id, permissions)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;

        Ok(LoginVo {
            token: token.as_str().to_string(),
        })
    }

    /// 登出
    pub async fn logout(&self, login_id: &str) -> ApiResult<()> {
        StpUtil::logout_by_login_id(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }

    /// 获取用户的按钮权限标识列表
    async fn get_user_permissions(&self, user_id: i64) -> ApiResult<Vec<String>> {
        use model::entity::sys_role_menu;

        let role_ids: Vec<i64> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(user_id))
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .map(|ur| ur.role_id)
            .collect();

        if role_ids.is_empty() {
            return Ok(vec![]);
        }

        let menus = sys_menu::Entity::find()
            .join(JoinType::InnerJoin, sys_menu::Relation::SysRoleMenu.def())
            .filter(sys_role_menu::Column::RoleId.is_in(role_ids))
            .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Button))
            .filter(sys_menu::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询菜单权限失败")?;

        Ok(menus
            .into_iter()
            .filter(|m| !m.auth_mark.is_empty())
            .map(|m| m.auth_mark)
            .collect())
    }
}
