use summer_admin_macros::log;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_system_model::dto::monitor::{CacheDeleteQuery, CacheKeysQuery};
use summer_system_model::vo::monitor::{CacheInfoVo, CacheKeyDetailVo, CacheKeysVo, ServerInfoVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api};

use crate::service::monitor_service::{CacheMonitorService, ServerMonitorService};

// ─── 服务监控 ────────────────────────────────────────────────────────────────

#[log(module = "服务监控", action = "查询服务器信息", biz_type = Query)]
#[get_api("/monitor/server")]
pub async fn server_info(
    Component(svc): Component<ServerMonitorService>,
) -> ApiResult<Json<ServerInfoVo>> {
    let vo = svc.get_server_info().await?;
    Ok(Json(vo))
}

// ─── 缓存监控 ────────────────────────────────────────────────────────────────

#[log(module = "缓存监控", action = "查询缓存信息", biz_type = Query)]
#[get_api("/monitor/cache/info")]
pub async fn cache_info(
    Component(svc): Component<CacheMonitorService>,
) -> ApiResult<Json<CacheInfoVo>> {
    let vo = svc.get_cache_info().await?;
    Ok(Json(vo))
}

#[log(module = "缓存监控", action = "查询缓存键列表", biz_type = Query)]
#[get_api("/monitor/cache/keys")]
pub async fn cache_keys(
    Component(svc): Component<CacheMonitorService>,
    Query(query): Query<CacheKeysQuery>,
) -> ApiResult<Json<CacheKeysVo>> {
    let vo = svc.get_cache_keys(query).await?;
    Ok(Json(vo))
}

#[log(module = "缓存监控", action = "查询缓存键详情", biz_type = Query)]
#[get_api("/monitor/cache/keys/{key}/value")]
pub async fn cache_key_detail(
    Component(svc): Component<CacheMonitorService>,
    Path(key): Path<String>,
) -> ApiResult<Json<CacheKeyDetailVo>> {
    let vo = svc.get_cache_key_detail(&key).await?;
    Ok(Json(vo))
}

#[log(module = "缓存监控", action = "删除缓存键", biz_type = Delete)]
#[delete_api("/monitor/cache/keys/{key}")]
pub async fn delete_cache_key(
    Component(svc): Component<CacheMonitorService>,
    Path(key): Path<String>,
) -> ApiResult<()> {
    svc.delete_cache_key(&key).await?;
    Ok(())
}

#[log(module = "缓存监控", action = "批量删除缓存键", biz_type = Delete)]
#[delete_api("/monitor/cache/keys")]
pub async fn delete_cache_keys_by_pattern(
    Component(svc): Component<CacheMonitorService>,
    Query(query): Query<CacheDeleteQuery>,
) -> ApiResult<()> {
    svc.delete_cache_keys_by_pattern(query).await?;
    Ok(())
}
