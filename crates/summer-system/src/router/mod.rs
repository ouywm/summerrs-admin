pub mod auth;
pub mod job;
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
pub mod sys_resource;
pub mod sys_role;
pub mod sys_user;
pub mod user_notice;
pub mod user_profile;

use summer_auth::{AuthLayer, ResourcePermissionLayer};
use summer_web::Router;

/// 组装 system 域 Router,挂上 JWT 鉴权和资源权限 layer。
pub fn router_with_layers(router: Router) -> Router {
    let group = crate::system_group();

    router
        .layer(ResourcePermissionLayer::new())
        .layer(AuthLayer::for_group(group))
}
