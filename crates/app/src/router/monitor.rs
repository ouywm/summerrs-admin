use common::error::ApiResult;
use common::extractor::{Path, Query};
use common::response::ApiResponse;
use macros::log;
use model::dto::monitor::{CacheDeleteQuery, CacheKeysQuery};
use model::vo::monitor::{CacheInfoVo, CacheKeyDetailVo, CacheKeysVo, ServerInfoVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api};

use crate::service::monitor_service::{CacheMonitorService, ServerMonitorService};

// ─── 服务监控 ────────────────────────────────────────────────────────────────

#[get_api("/monitor/server")]
#[log(module = "服务监控", action = "查询服务器信息", biz_type = Query)]
pub async fn server_info(
    Component(svc): Component<ServerMonitorService>,
) -> ApiResult<ApiResponse<ServerInfoVo>> {
    let vo = svc.get_server_info().await?;
    Ok(ApiResponse::ok(vo))
}

// ─── 缓存监控 ────────────────────────────────────────────────────────────────

#[log(module = "缓存监控", action = "查询缓存信息", biz_type = Query)]
#[get_api("/monitor/cache/info")]
pub async fn cache_info(
    Component(svc): Component<CacheMonitorService>,
) -> ApiResult<ApiResponse<CacheInfoVo>> {
    let vo = svc.get_cache_info().await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "缓存监控", action = "查询缓存键列表", biz_type = Query)]
#[get_api("/monitor/cache/keys")]
pub async fn cache_keys(
    Component(svc): Component<CacheMonitorService>,
    Query(query): Query<CacheKeysQuery>,
) -> ApiResult<ApiResponse<CacheKeysVo>> {
    let vo = svc.get_cache_keys(query).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "缓存监控", action = "查询缓存键详情", biz_type = Query)]
#[get_api("/monitor/cache/keys/{key}/value")]
pub async fn cache_key_detail(
    Component(svc): Component<CacheMonitorService>,
    Path(key): Path<String>,
) -> ApiResult<ApiResponse<CacheKeyDetailVo>> {
    let vo = svc.get_cache_key_detail(&key).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "缓存监控", action = "删除缓存键", biz_type = Delete)]
#[delete_api("/monitor/cache/keys/{key}")]
pub async fn delete_cache_key(
    Component(svc): Component<CacheMonitorService>,
    Path(key): Path<String>,
) -> ApiResult<ApiResponse<()>> {
    svc.delete_cache_key(&key).await?;
    Ok(ApiResponse::empty_with_msg("删除成功"))
}

#[log(module = "缓存监控", action = "批量删除缓存键", biz_type = Delete)]
#[delete_api("/monitor/cache/keys")]
pub async fn delete_cache_keys_by_pattern(
    Component(svc): Component<CacheMonitorService>,
    Query(query): Query<CacheDeleteQuery>,
) -> ApiResult<ApiResponse<()>> {
    let total = svc.delete_cache_keys_by_pattern(query).await?;
    Ok(ApiResponse::empty_with_msg(format!(
        "批量删除完成，共删除 {total} 个键"
    )))
}
