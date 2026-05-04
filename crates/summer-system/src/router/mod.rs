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
pub mod sys_tenant;
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

pub fn admin_router() -> Router {
    let router = Router::new();
    let router = auth::routes(router);
    let router = login_log::routes(router);
    let router = monitor::routes(router);
    let router = online::routes(router);
    let router = operation_log::routes(router);
    let router = public_file::routes(router);
    let router = sys_config::routes(router);
    let router = sys_config_group::routes(router);
    let router = sys_dict::routes(router);
    let router = sys_file::routes(router);
    let router = sys_file_folder::routes(router);
    let router = sys_file_upload::routes(router);
    let router = sys_menu::routes(router);
    let router = sys_notice::routes(router);
    let router = sys_role::routes(router);
    let router = sys_tenant::routes(router);
    let router = sys_user::routes(router);
    let router = user_notice::routes(router);
    user_profile::routes(router)
}
