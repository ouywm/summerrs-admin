use crate::service::token_service::TokenService;
use summer_admin_macros::log;
use summer_ai_model::dto::token::{
    CreateTokenDto, TokenQueryDto, UpdateTokenDto, UpdateTokenStatusDto,
};
use summer_ai_model::vo::token::{CreatedTokenVo, RotatedTokenKeyVo, TokenDetailVo, TokenVo};
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/令牌管理", action = "查询令牌列表", biz_type = Query)]
#[get_api("/token/list")]
pub async fn list(
    Component(svc): Component<TokenService>,
    Query(query): Query<TokenQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<TokenVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/令牌管理", action = "查询令牌详情", biz_type = Query)]
#[get_api("/token/{id}")]
pub async fn detail(
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<TokenDetailVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(
    module = "ai/令牌管理",
    action = "创建令牌",
    biz_type = Create,
    save_response = false
)]
#[post_api("/token")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<TokenService>,
    ValidatedJson(dto): ValidatedJson<CreateTokenDto>,
) -> ApiResult<Json<CreatedTokenVo>> {
    let vo = svc.create(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[log(module = "ai/令牌管理", action = "更新令牌", biz_type = Update)]
#[put_api("/token/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateTokenDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/令牌管理", action = "更新令牌状态", biz_type = Update)]
#[put_api("/token/{id}/status")]
pub async fn update_status(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateTokenStatusDto>,
) -> ApiResult<()> {
    svc.update_status(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(
    module = "ai/令牌管理",
    action = "轮换令牌密钥",
    biz_type = Update,
    save_response = false
)]
#[post_api("/token/{id}/rotate-key")]
pub async fn rotate_key(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RotatedTokenKeyVo>> {
    let vo = svc.rotate_key(id, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[log(module = "ai/令牌管理", action = "删除令牌", biz_type = Delete)]
#[delete_api("/token/{id}")]
pub async fn delete(Component(svc): Component<TokenService>, Path(id): Path<i64>) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

#[derive(
    Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema, validator::Validate,
)]
#[serde(rename_all = "camelCase")]
pub struct BatchDeleteTokenDto {
    #[validate(length(min = 1, message = "ids不能为空"))]
    pub ids: Vec<i64>,
}

#[log(module = "ai/令牌管理", action = "批量删除令牌", biz_type = Delete)]
#[post_api("/token/batch-delete")]
pub async fn batch_delete(
    Component(svc): Component<TokenService>,
    ValidatedJson(dto): ValidatedJson<BatchDeleteTokenDto>,
) -> ApiResult<Json<serde_json::Value>> {
    let count = svc.batch_delete(dto.ids).await?;
    Ok(Json(serde_json::json!({ "deleted": count })))
}
