//! Generated admin router skeleton.

use common::error::ApiResult;
use common::extractor::{Path, Query, ValidatedJson};
use common::response::Json;
use macros::log;
use model::dto::sys_config::{ConfigQueryDto, CreateConfigDto, UpdateConfigDto};
use model::vo::sys_config::ConfigVo;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_config_service::SysConfigService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "系统参数配置表", action = "查询列表", biz_type = Query)]
#[get_api("/config/list")]
pub async fn list(
    Component(svc): Component<SysConfigService>,
    Query(query): Query<ConfigQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ConfigVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "系统参数配置表", action = "查询详情", biz_type = Query)]
#[get_api("/config/{id}")]
pub async fn detail(
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConfigVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "系统参数配置表", action = "创建", biz_type = Create)]
#[post_api("/config")]
pub async fn create(
    Component(svc): Component<SysConfigService>,
    ValidatedJson(dto): ValidatedJson<CreateConfigDto>,
) -> ApiResult<()> {
    svc.create(dto).await?;
    Ok(())
}

#[log(module = "系统参数配置表", action = "更新", biz_type = Update)]
#[put_api("/config/{id}")]
pub async fn update(
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConfigDto>,
) -> ApiResult<()> {
    svc.update(id, dto).await?;
    Ok(())
}

#[log(module = "系统参数配置表", action = "删除", biz_type = Delete)]
#[delete_api("/config/{id}")]
pub async fn delete(
    Component(svc): Component<SysConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
