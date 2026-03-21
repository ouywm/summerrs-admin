use anyhow::Context;
use summer_common::crypto::verify_password;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::user_agent::UserAgentInfo;
use summer_model::dto::auth::LoginDto;
use summer_model::entity::sys_login_log;
use summer_model::entity::sys_menu;
use summer_model::entity::sys_role;
use summer_model::entity::sys_user;
use summer_model::entity::sys_user_role;
use summer_model::entity::{biz_role, biz_user, biz_user_role, customer};
use summer_model::vo::auth::{DeviceSessionVo, LoginVo};
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait};
use std::net::IpAddr;
use summer::plugin::Service;
use summer_auth::{
    AdminProfile, BusinessProfile, CustomerProfile, DeviceType, LoginId, LoginParams,
    SessionManager, UserProfile, UserType,
};

use crate::service::login_log_service::LoginLogService;
use summer_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct AuthService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    login_log_service: LoginLogService,
    #[inject(component)]
    auth: SessionManager,
}

impl AuthService {
    /// Admin 登录（原 login 方法）
    pub async fn admin_login(
        &self,
        dto: LoginDto,
        client_ip: IpAddr,
        ua_info: UserAgentInfo,
    ) -> ApiResult<LoginVo> {
        // 根据用户名查询用户
        let user = sys_user::Entity::find()
            .filter(sys_user::Column::UserName.eq(&dto.user_name))
            .one(&self.db)
            .await
            .context("查询用户失败")?;

        // 用户不存在
        if user.is_none() {
            self.login_log_service.record_login_async(
                0,
                dto.user_name.clone(),
                client_ip,
                ua_info,
                sys_login_log::LoginStatus::Failed,
                Some("用户不存在".to_string()),
            );
            return Err(ApiErrors::Unauthorized("用户名或密码错误".to_string()));
        }

        let user = user.unwrap();

        // 验证用户状态
        if user.status == sys_user::UserStatus::Disabled {
            self.login_log_service.record_login_async(
                user.id,
                user.user_name.clone(),
                client_ip,
                ua_info,
                sys_login_log::LoginStatus::Failed,
                Some("账号已被禁用".to_string()),
            );
            return Err(ApiErrors::Forbidden("账号已被禁用".to_string()));
        }

        // 验证密码
        let valid = verify_password(&dto.password, &user.password)
            .map_err(|_| ApiErrors::Unauthorized("用户名或密码错误".to_string()))?;
        if !valid {
            self.login_log_service.record_login_async(
                user.id,
                user.user_name.clone(),
                client_ip,
                ua_info,
                sys_login_log::LoginStatus::Failed,
                Some("密码错误".to_string()),
            );
            return Err(ApiErrors::Unauthorized("用户名或密码错误".to_string()));
        }

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

        // 登录并获取 TokenPair
        let login_id = LoginId::admin(user.id);
        let token_pair = self
            .auth
            .login(LoginParams {
                login_id,
                device: DeviceType::Web,
                login_ip: client_ip.to_string(),
                user_agent: ua_info.raw.clone(),
                profile: UserProfile::Admin(AdminProfile {
                    user_name: user.user_name.clone(),
                    nick_name: user.nick_name.clone(),
                    roles,
                    permissions,
                }),
            })
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;

        // 异步记录登录成功日志
        self.login_log_service.record_login_async(
            user.id,
            user.user_name.clone(),
            client_ip,
            ua_info,
            sys_login_log::LoginStatus::Success,
            None,
        );

        Ok(LoginVo {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            expires_in: token_pair.expires_in,
        })
    }

    /// 登出
    pub async fn logout(&self, login_id: &LoginId, device: &DeviceType) -> ApiResult<()> {
        self.auth
            .logout(login_id, device)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }

    /// 刷新 Token
    pub async fn refresh_token(&self, refresh_token: &str) -> ApiResult<LoginVo> {
        // 1. 先解析 refresh JWT 拿到 login_id（不查 Redis）
        let login_id = self
            .auth
            .parse_refresh_token(refresh_token)
            .map_err(|e| ApiErrors::Unauthorized(e.to_string()))?;

        // 2. 根据用户类型从 DB 查询最新 profile
        let profile = self.load_user_profile(&login_id).await?;

        // 3. 调用 refresh（会验证 Redis 中 refresh key + deny check + 轮转）
        let pair = self
            .auth
            .refresh(refresh_token, &profile)
            .await
            .map_err(|e| ApiErrors::Unauthorized(e.to_string()))?;

        Ok(LoginVo {
            access_token: pair.access_token,
            refresh_token: pair.refresh_token,
            expires_in: pair.expires_in,
        })
    }

    /// 登出所有设备
    pub async fn logout_all(&self, login_id: &LoginId) -> ApiResult<()> {
        self.auth
            .logout_all(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }

    /// 获取当前用户的所有设备会话
    pub async fn get_sessions(&self, login_id: &LoginId) -> ApiResult<Vec<DeviceSessionVo>> {
        let devices = self
            .auth
            .get_devices(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;

        Ok(devices
            .into_iter()
            .map(|d| {
                let ua = UserAgentInfo::parse(&d.user_agent);
                DeviceSessionVo {
                    device: d.device.to_string(),
                    login_time: d.login_time,
                    login_ip: d.login_ip,
                    browser: ua.browser,
                    os: ua.os,
                }
            })
            .collect())
    }

    /// 踢下指定设备
    pub async fn kick_device(&self, login_id: &LoginId, device: DeviceType) -> ApiResult<()> {
        self.auth
            .kick_out(login_id, Some(&device))
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    }

    /// 获取用户的按钮权限标识列表
    async fn get_user_permissions(&self, user_id: i64) -> ApiResult<Vec<String>> {
        use summer_model::entity::sys_role_menu;

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

    /// 根据 login_id 从 DB 加载最新的用户 Profile（refresh 时使用）
    async fn load_user_profile(&self, login_id: &LoginId) -> ApiResult<UserProfile> {
        match login_id.user_type {
            UserType::Admin => {
                let user = sys_user::Entity::find_by_id(login_id.user_id)
                    .one(&self.db)
                    .await
                    .context("查询管理员失败")?
                    .ok_or_else(|| ApiErrors::Unauthorized("用户不存在".to_string()))?;

                let roles: Vec<String> = sys_user_role::Entity::find()
                    .filter(sys_user_role::Column::UserId.eq(user.id))
                    .find_also_related(sys_role::Entity)
                    .all(&self.db)
                    .await
                    .context("查询用户角色失败")?
                    .into_iter()
                    .filter_map(|(_, role)| role.map(|r| r.role_code))
                    .collect();

                let permissions = self.get_user_permissions(user.id).await?;

                Ok(UserProfile::Admin(AdminProfile {
                    user_name: user.user_name,
                    nick_name: user.nick_name,
                    roles,
                    permissions,
                }))
            }
            UserType::Business => {
                let user = biz_user::Entity::find_by_id(login_id.user_id)
                    .one(&self.db)
                    .await
                    .context("查询B端用户失败")?
                    .ok_or_else(|| ApiErrors::Unauthorized("用户不存在".to_string()))?;

                let roles: Vec<String> = biz_user_role::Entity::find()
                    .filter(biz_user_role::Column::UserId.eq(user.id))
                    .find_also_related(biz_role::Entity)
                    .all(&self.db)
                    .await
                    .context("查询B端用户角色失败")?
                    .into_iter()
                    .filter_map(|(_, role)| role.map(|r| r.role_code))
                    .collect();

                Ok(UserProfile::Business(BusinessProfile {
                    user_name: user.user_name,
                    nick_name: user.nick_name,
                    roles,
                    permissions: vec![],
                }))
            }
            UserType::Customer => {
                let user = customer::Entity::find_by_id(login_id.user_id)
                    .one(&self.db)
                    .await
                    .context("查询C端用户失败")?
                    .ok_or_else(|| ApiErrors::Unauthorized("用户不存在".to_string()))?;

                Ok(UserProfile::Customer(CustomerProfile {
                    nick_name: user.nick_name,
                }))
            }
        }
    }
}
