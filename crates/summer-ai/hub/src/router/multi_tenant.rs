use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::multi_tenant::MultiTenantService;
use summer_ai_model::dto::multi_tenant::*;
use summer_ai_model::vo::multi_tenant::*;

// ─── Organization ───
#[get_api("/ai/organization")]
pub async fn list_orgs(
    Component(svc): Component<MultiTenantService>,
    Query(q): Query<QueryOrganizationDto>,
    p: Pagination,
) -> ApiResult<Json<Page<OrganizationVo>>> {
    Ok(Json(svc.list_orgs(q, p).await?))
}
#[get_api("/ai/organization/{id}")]
pub async fn get_org(
    Component(svc): Component<MultiTenantService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<OrganizationVo>> {
    Ok(Json(svc.get_org(id).await?))
}
#[post_api("/ai/organization")]
pub async fn create_org(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    ValidatedJson(dto): ValidatedJson<CreateOrganizationDto>,
) -> ApiResult<Json<OrganizationVo>> {
    Ok(Json(svc.create_org(dto, &profile.nick_name).await?))
}
#[put_api("/ai/organization/{id}")]
pub async fn update_org(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateOrganizationDto>,
) -> ApiResult<Json<OrganizationVo>> {
    Ok(Json(svc.update_org(id, dto, &profile.nick_name).await?))
}
#[delete_api("/ai/organization/{id}")]
pub async fn delete_org(
    Component(svc): Component<MultiTenantService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_org(id).await
}

// ─── Team ───
#[get_api("/ai/team")]
pub async fn list_teams(
    Component(svc): Component<MultiTenantService>,
    Query(q): Query<QueryTeamDto>,
    p: Pagination,
) -> ApiResult<Json<Page<TeamVo>>> {
    Ok(Json(svc.list_teams(q, p).await?))
}
#[post_api("/ai/team")]
pub async fn create_team(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    ValidatedJson(dto): ValidatedJson<CreateTeamDto>,
) -> ApiResult<Json<TeamVo>> {
    Ok(Json(svc.create_team(dto, &profile.nick_name).await?))
}
#[put_api("/ai/team/{id}")]
pub async fn update_team(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateTeamDto>,
) -> ApiResult<Json<TeamVo>> {
    Ok(Json(svc.update_team(id, dto, &profile.nick_name).await?))
}

// ─── Project ───
#[get_api("/ai/project")]
pub async fn list_projects(
    Component(svc): Component<MultiTenantService>,
    Query(q): Query<QueryProjectDto>,
    p: Pagination,
) -> ApiResult<Json<Page<ProjectVo>>> {
    Ok(Json(svc.list_projects(q, p).await?))
}
#[post_api("/ai/project")]
pub async fn create_project(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    ValidatedJson(dto): ValidatedJson<CreateProjectDto>,
) -> ApiResult<Json<ProjectVo>> {
    Ok(Json(svc.create_project(dto, &profile.nick_name).await?))
}
#[put_api("/ai/project/{id}")]
pub async fn update_project(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<MultiTenantService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateProjectDto>,
) -> ApiResult<Json<ProjectVo>> {
    Ok(Json(svc.update_project(id, dto, &profile.nick_name).await?))
}

// ─── AuditLog ───
#[get_api("/ai/audit-log")]
pub async fn list_audit_logs(
    Component(svc): Component<MultiTenantService>,
    Query(q): Query<QueryAuditLogDto>,
    p: Pagination,
) -> ApiResult<Json<Page<AuditLogVo>>> {
    Ok(Json(svc.list_audit_logs(q, p).await?))
}
