use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::audit_log;
use crate::entity::organization;
use crate::entity::project;
use crate::entity::team;

// ─── Organization ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrganizationDto {
    #[validate(length(min = 1, max = 64))]
    pub org_code: String,
    #[validate(length(min = 1, max = 128))]
    pub org_name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub owner_user_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrganizationDto {
    pub org_name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub status: Option<i16>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryOrganizationDto {
    pub org_code: Option<String>,
    pub org_name: Option<String>,
    pub status: Option<i16>,
}

impl From<QueryOrganizationDto> for sea_orm::Condition {
    fn from(dto: QueryOrganizationDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all().add(organization::Column::DeletedAt.is_null());
        if let Some(v) = dto.org_code {
            cond = cond.add(organization::Column::OrgCode.eq(v));
        }
        if let Some(v) = dto.org_name {
            cond = cond.add(organization::Column::OrgName.contains(&v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(organization::Column::Status.eq(v));
        }
        cond
    }
}

// ─── Team ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateTeamDto {
    pub organization_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub team_code: String,
    pub team_name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTeamDto {
    pub team_name: Option<String>,
    pub description: Option<String>,
    pub status: Option<i16>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryTeamDto {
    pub organization_id: Option<i64>,
    pub team_name: Option<String>,
}

impl From<QueryTeamDto> for sea_orm::Condition {
    fn from(dto: QueryTeamDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all().add(team::Column::DeletedAt.is_null());
        if let Some(v) = dto.organization_id {
            cond = cond.add(team::Column::OrganizationId.eq(v));
        }
        if let Some(v) = dto.team_name {
            cond = cond.add(team::Column::TeamName.contains(&v));
        }
        cond
    }
}

// ─── Project ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectDto {
    pub organization_id: i64,
    #[serde(default)]
    pub team_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub project_code: String,
    pub project_name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectDto {
    pub project_name: Option<String>,
    pub description: Option<String>,
    pub status: Option<i16>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryProjectDto {
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub project_name: Option<String>,
}

impl From<QueryProjectDto> for sea_orm::Condition {
    fn from(dto: QueryProjectDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all().add(project::Column::DeletedAt.is_null());
        if let Some(v) = dto.organization_id {
            cond = cond.add(project::Column::OrganizationId.eq(v));
        }
        if let Some(v) = dto.team_id {
            cond = cond.add(project::Column::TeamId.eq(v));
        }
        if let Some(v) = dto.project_name {
            cond = cond.add(project::Column::ProjectName.contains(&v));
        }
        cond
    }
}

// ─── AuditLog Query ───

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryAuditLogDto {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
}

impl From<QueryAuditLogDto> for sea_orm::Condition {
    fn from(dto: QueryAuditLogDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.organization_id {
            cond = cond.add(audit_log::Column::OrganizationId.eq(v));
        }
        if let Some(v) = dto.project_id {
            cond = cond.add(audit_log::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.user_id {
            cond = cond.add(audit_log::Column::UserId.eq(v));
        }
        if let Some(v) = dto.action {
            cond = cond.add(audit_log::Column::Action.eq(v));
        }
        if let Some(v) = dto.resource_type {
            cond = cond.add(audit_log::Column::ResourceType.eq(v));
        }
        cond
    }
}
