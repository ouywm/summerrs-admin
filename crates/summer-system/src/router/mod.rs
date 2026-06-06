pub mod auth;
pub mod login_log;
pub mod monitor;
pub mod online;
pub mod operation_log;
pub mod public_file;
pub mod sys_config;
pub mod sys_config_group;
pub mod sys_dict;
pub mod sys_file;
pub mod sys_file_folder;
pub mod sys_file_upload;
pub mod sys_menu;
pub mod sys_notice;
pub mod sys_role;
pub mod sys_user;
pub mod user_notice;
pub mod user_profile;

use summer_auth::{GroupAuthLayer, JwtStrategy};
use summer_web::Router;

#[derive(Clone)]
pub struct SystemAdminRouteGroup(pub Router);

/// 组装 system 域 Router,挂上 JWT 鉴权 layer。
///
/// app crate 直接调这个函数即可,不需要 import [`JwtStrategy`]。inventory 注册的
/// 全部 system handler 都属于 [`crate::system_group`],一次 `grouped_router` 拿全。
pub fn router_with_layers() -> Router {
    summer_web::handler::grouped_router(crate::system_group()).layer(GroupAuthLayer::new(
        JwtStrategy::for_group(crate::system_group()),
    ))
}
