pub mod req;
pub mod res;

use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::get_api;
use summer_web::handler::TypeRouter;

use crate::service::retry_attempt::RetryAttemptService;

use self::req::RetryAttemptQuery;
use self::res::{RetryAttemptDetailRes, RetryAttemptListRes};

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_retry_attempts)
        .typed_route(get_retry_attempt)
}

#[get_api("/ai/retry-attempt/list")]
pub async fn list_retry_attempts(
    Component(svc): Component<RetryAttemptService>,
    Query(query): Query<RetryAttemptQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RetryAttemptListRes>>> {
    let page = svc.list_retry_attempts(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/retry-attempt/{id}")]
pub async fn get_retry_attempt(
    Component(svc): Component<RetryAttemptService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RetryAttemptDetailRes>> {
    let detail = svc.get_retry_attempt(id).await?;
    Ok(Json(detail))
}
