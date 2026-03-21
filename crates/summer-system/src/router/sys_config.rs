use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_admin_macros::log;
use summer_model::dto::sys_config::{
    ConfigGroupFilterQueryDto, ConfigKeysDto, ConfigQueryDto, CreateConfigDto, UpdateConfigDto,
};
use summer_model::vo::sys_config::{ConfigDetailVo, ConfigGroupBlockVo, ConfigValueVo};
use std::collections::HashMap;
use summer_auth::AdminUser;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_config_service::SysConfigService;

#[log(module = "系统参数配置", action = "按分组查询列表", biz_type = Query)]
#[get_api("/config/grouped")]
pub async fn grouped(
    Component(svc): Component<SysConfigService>,
    Query(config): Query<ConfigQueryDto>,
    Query(group): Query<ConfigGroupFilterQueryDto>,
) -> ApiResult<Json<Vec<ConfigGroupBlockVo>>> {
    let groups = svc.grouped(config, group).await?;
    Ok(Json(groups))
}

#[log(module = "系统参数配置", action = "根据配置键获取配置", biz_type = Query, save_params = false)]
#[get_api("/config/by-key/{config_key}")]
pub async fn get_by_key(
    Component(svc): Component<SysConfigService>,
    Path(config_key): Path<String>,
) -> ApiResult<Json<ConfigValueVo>> {
    let item = svc.get_by_key(&config_key).await?;
    Ok(Json(item))
}

#[log(module = "系统参数配置", action = "批量获取配置", biz_type = Query, save_params = false)]
#[post_api("/config/by-keys")]
pub async fn get_by_keys(
    Component(svc): Component<SysConfigService>,
    ValidatedJson(dto): ValidatedJson<ConfigKeysDto>,
) -> ApiResult<Json<HashMap<String, ConfigValueVo>>> {
    let items = svc.get_by_keys(dto).await?;
    Ok(Json(items))
}

#[log(module = "系统参数配置", action = "查询详情", biz_type = Query)]
#[get_api("/config/{id}")]
pub async fn detail(
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConfigDetailVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "系统参数配置", action = "创建", biz_type = Create)]
#[post_api("/config")]
pub async fn create(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysConfigService>,
    ValidatedJson(dto): ValidatedJson<CreateConfigDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统参数配置", action = "更新", biz_type = Update)]
#[put_api("/config/{id}")]
pub async fn update(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConfigDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统参数配置", action = "删除", biz_type = Delete)]
#[delete_api("/config/{id}")]
pub async fn delete(
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
