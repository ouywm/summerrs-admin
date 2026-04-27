use crate::service::user_quota_service::UserQuotaService;
use summer_admin_macros::log;
use summer_ai_model::dto::user_quota::{
    AdjustUserQuotaDto, CreateUserQuotaDto, UpdateUserQuotaDto, UserQuotaQueryDto,
};
use summer_ai_model::vo::user_quota::UserQuotaVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{get_api, post_api, put_api};

#[log(module = "ai/用户额度管理", action = "查询用户额度列表", biz_type = Query)]
#[get_api("/user-quota/list")]
pub async fn list(
    Component(svc): Component<UserQuotaService>,
    Query(query): Query<UserQuotaQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<UserQuotaVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/用户额度管理", action = "查询用户额度详情", biz_type = Query)]
#[get_api("/user-quota/{id}")]
pub async fn detail(
    Component(svc): Component<UserQuotaService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<UserQuotaVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/用户额度管理", action = "创建用户额度", biz_type = Create)]
#[post_api("/user-quota")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<UserQuotaService>,
    ValidatedJson(dto): ValidatedJson<CreateUserQuotaDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/用户额度管理", action = "更新用户额度", biz_type = Update)]
#[put_api("/user-quota/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<UserQuotaService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateUserQuotaDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/用户额度管理", action = "调整用户额度", biz_type = Update)]
#[post_api("/user-quota/{id}/adjust")]
pub async fn adjust(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<UserQuotaService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<AdjustUserQuotaDto>,
) -> ApiResult<()> {
    svc.adjust(id, dto, &profile.nick_name).await?;
    Ok(())
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(detail)
        .typed_route(create)
        .typed_route(update)
        .typed_route(adjust)
}
