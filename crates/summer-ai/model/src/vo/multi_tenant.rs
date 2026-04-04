use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::{audit_log, organization, project, team};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationVo {
    pub id: i64,
    pub org_code: String,
    pub org_name: String,
    pub display_name: String,
    pub logo_url: String,
    pub description: String,
    pub owner_user_id: i64,
    pub status: i16,
    pub settings: serde_json::Value,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}
impl OrganizationVo {
    pub fn from_model(m: organization::Model) -> Self {
        Self {
            id: m.id,
            org_code: m.org_code,
            org_name: m.org_name,
            display_name: m.display_name,
            logo_url: m.logo_url,
            description: m.description,
            owner_user_id: m.owner_user_id,
            status: m.status,
            settings: m.settings,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamVo {
    pub id: i64,
    pub organization_id: i64,
    pub team_code: String,
    pub team_name: String,
    pub description: String,
    pub status: i16,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}
impl TeamVo {
    pub fn from_model(m: team::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            team_code: m.team_code,
            team_name: m.team_name,
            description: m.description,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVo {
    pub id: i64,
    pub organization_id: i64,
    pub team_id: i64,
    pub project_code: String,
    pub project_name: String,
    pub description: String,
    pub status: i16,
    pub settings: serde_json::Value,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}
impl ProjectVo {
    pub fn from_model(m: project::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            team_id: m.team_id,
            project_code: m.project_code,
            project_name: m.project_name,
            description: m.description,
            status: m.status,
            settings: m.settings,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogVo {
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub change_set: serde_json::Value,
    pub client_ip: String,
    pub create_time: DateTime<FixedOffset>,
}
impl AuditLogVo {
    pub fn from_model(m: audit_log::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            project_id: m.project_id,
            user_id: m.user_id,
            action: m.action,
            resource_type: m.resource_type,
            resource_id: m.resource_id,
            change_set: m.change_set,
            client_ip: m.client_ip,
            create_time: m.create_time,
        }
    }
}
