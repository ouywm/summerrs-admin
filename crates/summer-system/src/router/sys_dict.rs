use std::collections::HashMap;
use summer_admin_macros::log;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::sys_dict::{
    CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto, UpdateDictDataDto,
    UpdateDictTypeDto,
};
use summer_system_model::vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_dict_service::SysDictService;
use summer_sea_orm::pagination::{Page, Pagination};

// ============================================================
// 字典类型路由
// ============================================================

#[log(module = "字典管理", action = "查询字典类型列表", biz_type = Query)]
#[get_api("/dict/type/list")]
pub async fn list_dict_types(
    Component(svc): Component<SysDictService>,
    Query(query): Query<DictTypeQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<DictTypeVo>>> {
    let vo = svc.list_dict_types(query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "字典管理", action = "创建字典类型", biz_type = Create)]
#[post_api("/dict/type")]
pub async fn create_dict_type(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysDictService>,
    ValidatedJson(dto): ValidatedJson<CreateDictTypeDto>,
) -> ApiResult<()> {
    svc.create_dict_type(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "字典管理", action = "更新字典类型", biz_type = Update)]
#[put_api("/dict/type/{id}")]
pub async fn update_dict_type(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysDictService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateDictTypeDto>,
) -> ApiResult<()> {
    svc.update_dict_type(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "字典管理", action = "删除字典类型", biz_type = Delete)]
#[delete_api("/dict/type/{id}")]
pub async fn delete_dict_type(
    Component(svc): Component<SysDictService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_dict_type(id).await?;
    Ok(())
}

// ============================================================
// 字典数据路由
// ============================================================

#[log(module = "字典管理", action = "查询字典数据列表", biz_type = Query)]
#[get_api("/dict/data/list")]
pub async fn list_dict_data(
    Component(svc): Component<SysDictService>,
    Query(query): Query<DictDataQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<DictDataVo>>> {
    let vo = svc.list_dict_data(query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "字典管理", action = "根据类型获取字典数据", biz_type = Query, save_params = false)]
#[get_api("/dict/data/by-type/{dict_type}")]
pub async fn get_dict_data_by_type(
    Component(svc): Component<SysDictService>,
    Path(dict_type): Path<String>,
) -> ApiResult<Json<Vec<DictDataSimpleVo>>> {
    let vo = svc.get_dict_data_by_type(&dict_type).await?;
    Ok(Json(vo))
}

#[log(module = "字典管理", action = "获取全量字典数据", biz_type = Query, save_params = false)]
#[get_api("/dict/all")]
pub async fn get_all_dict_data(
    Component(svc): Component<SysDictService>,
) -> ApiResult<Json<HashMap<String, Vec<DictDataSimpleVo>>>> {
    let vo = svc.get_all_dict_data().await?;
    Ok(Json(vo))
}

#[log(module = "字典管理", action = "创建字典数据", biz_type = Create)]
#[post_api("/dict/data")]
pub async fn create_dict_data(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysDictService>,
    ValidatedJson(dto): ValidatedJson<CreateDictDataDto>,
) -> ApiResult<()> {
    svc.create_dict_data(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "字典管理", action = "更新字典数据", biz_type = Update)]
#[put_api("/dict/data/{id}")]
pub async fn update_dict_data(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysDictService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateDictDataDto>,
) -> ApiResult<()> {
    svc.update_dict_data(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "字典管理", action = "删除字典数据", biz_type = Delete)]
#[delete_api("/dict/data/{id}")]
pub async fn delete_dict_data(
    Component(svc): Component<SysDictService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_dict_data(id).await?;
    Ok(())
}
