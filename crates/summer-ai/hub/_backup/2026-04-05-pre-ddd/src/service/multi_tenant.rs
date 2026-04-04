use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::multi_tenant::*;
use summer_ai_model::entity::{audit_log, organization, project, team};
use summer_ai_model::vo::multi_tenant::*;

#[derive(Clone, Service)]
pub struct MultiTenantService {
    #[inject(component)]
    db: DbConn,
}

impl MultiTenantService {
    // ─── Organization ───
    pub async fn list_orgs(
        &self,
        query: QueryOrganizationDto,
        pagination: Pagination,
    ) -> ApiResult<Page<OrganizationVo>> {
        let page = organization::Entity::find()
            .filter(query)
            .order_by_desc(organization::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询组织列表失败")?;
        Ok(page.map(OrganizationVo::from_model))
    }
    pub async fn get_org(&self, id: i64) -> ApiResult<OrganizationVo> {
        let m = organization::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询组织失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("组织不存在".to_string()))?;
        Ok(OrganizationVo::from_model(m))
    }
    pub async fn create_org(
        &self,
        dto: CreateOrganizationDto,
        operator: &str,
    ) -> ApiResult<OrganizationVo> {
        let now = chrono::Utc::now().fixed_offset();
        let active = organization::ActiveModel {
            org_code: Set(dto.org_code),
            org_name: Set(dto.org_name),
            display_name: Set(dto.display_name),
            logo_url: Set(String::new()),
            description: Set(dto.description),
            owner_user_id: Set(dto.owner_user_id),
            status: Set(1),
            settings: Set(serde_json::json!({})),
            metadata: Set(serde_json::json!({})),
            deleted_at: Set(None),
            create_by: Set(operator.into()),
            update_by: Set(operator.into()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let m = active
            .insert(&self.db)
            .await
            .context("创建组织失败")
            .map_err(ApiErrors::Internal)?;
        Ok(OrganizationVo::from_model(m))
    }
    pub async fn update_org(
        &self,
        id: i64,
        dto: UpdateOrganizationDto,
        operator: &str,
    ) -> ApiResult<OrganizationVo> {
        let m = organization::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询组织失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("组织不存在".to_string()))?;
        let mut a: organization::ActiveModel = m.into();
        if let Some(v) = dto.org_name {
            a.org_name = Set(v);
        }
        if let Some(v) = dto.display_name {
            a.display_name = Set(v);
        }
        if let Some(v) = dto.description {
            a.description = Set(v);
        }
        if let Some(v) = dto.logo_url {
            a.logo_url = Set(v);
        }
        if let Some(v) = dto.status {
            a.status = Set(v);
        }
        if let Some(v) = dto.settings {
            a.settings = Set(v);
        }
        a.update_by = Set(operator.into());
        let u = a
            .update(&self.db)
            .await
            .context("更新组织失败")
            .map_err(ApiErrors::Internal)?;
        Ok(OrganizationVo::from_model(u))
    }
    pub async fn delete_org(&self, id: i64) -> ApiResult<()> {
        let m = organization::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("组织不存在".to_string()))?;
        let mut a: organization::ActiveModel = m.into();
        a.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        a.update(&self.db)
            .await
            .context("删除组织失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── Team ───
    pub async fn list_teams(
        &self,
        query: QueryTeamDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TeamVo>> {
        let page = team::Entity::find()
            .filter(query)
            .order_by_desc(team::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询团队列表失败")?;
        Ok(page.map(TeamVo::from_model))
    }
    pub async fn create_team(&self, dto: CreateTeamDto, operator: &str) -> ApiResult<TeamVo> {
        let now = chrono::Utc::now().fixed_offset();
        let a = team::ActiveModel {
            organization_id: Set(dto.organization_id),
            team_code: Set(dto.team_code),
            team_name: Set(dto.team_name),
            description: Set(dto.description),
            status: Set(1),
            metadata: Set(serde_json::json!({})),
            deleted_at: Set(None),
            create_by: Set(operator.into()),
            update_by: Set(operator.into()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let m = a
            .insert(&self.db)
            .await
            .context("创建团队失败")
            .map_err(ApiErrors::Internal)?;
        Ok(TeamVo::from_model(m))
    }
    pub async fn update_team(
        &self,
        id: i64,
        dto: UpdateTeamDto,
        operator: &str,
    ) -> ApiResult<TeamVo> {
        let m = team::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("团队不存在".to_string()))?;
        let mut a: team::ActiveModel = m.into();
        if let Some(v) = dto.team_name {
            a.team_name = Set(v);
        }
        if let Some(v) = dto.description {
            a.description = Set(v);
        }
        if let Some(v) = dto.status {
            a.status = Set(v);
        }
        a.update_by = Set(operator.into());
        let u = a
            .update(&self.db)
            .await
            .context("更新团队失败")
            .map_err(ApiErrors::Internal)?;
        Ok(TeamVo::from_model(u))
    }

    // ─── Project ───
    pub async fn list_projects(
        &self,
        query: QueryProjectDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ProjectVo>> {
        let page = project::Entity::find()
            .filter(query)
            .order_by_desc(project::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询项目列表失败")?;
        Ok(page.map(ProjectVo::from_model))
    }
    pub async fn create_project(
        &self,
        dto: CreateProjectDto,
        operator: &str,
    ) -> ApiResult<ProjectVo> {
        let now = chrono::Utc::now().fixed_offset();
        let a = project::ActiveModel {
            organization_id: Set(dto.organization_id),
            team_id: Set(dto.team_id),
            project_code: Set(dto.project_code),
            project_name: Set(dto.project_name),
            description: Set(dto.description),
            status: Set(1),
            settings: Set(serde_json::json!({})),
            metadata: Set(serde_json::json!({})),
            deleted_at: Set(None),
            create_by: Set(operator.into()),
            update_by: Set(operator.into()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let m = a
            .insert(&self.db)
            .await
            .context("创建项目失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ProjectVo::from_model(m))
    }
    pub async fn update_project(
        &self,
        id: i64,
        dto: UpdateProjectDto,
        operator: &str,
    ) -> ApiResult<ProjectVo> {
        let m = project::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("项目不存在".to_string()))?;
        let mut a: project::ActiveModel = m.into();
        if let Some(v) = dto.project_name {
            a.project_name = Set(v);
        }
        if let Some(v) = dto.description {
            a.description = Set(v);
        }
        if let Some(v) = dto.status {
            a.status = Set(v);
        }
        if let Some(v) = dto.settings {
            a.settings = Set(v);
        }
        a.update_by = Set(operator.into());
        let u = a
            .update(&self.db)
            .await
            .context("更新项目失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ProjectVo::from_model(u))
    }

    // ─── AuditLog ───
    pub async fn list_audit_logs(
        &self,
        query: QueryAuditLogDto,
        pagination: Pagination,
    ) -> ApiResult<Page<AuditLogVo>> {
        let page = audit_log::Entity::find()
            .filter(query)
            .order_by_desc(audit_log::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询审计日志失败")?;
        Ok(page.map(AuditLogVo::from_model))
    }
}
