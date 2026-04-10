use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::sys_role;

/// 角色信息（用于用户详情）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetailVo {
    pub role_id: i64,
    pub role_name: String,
    pub role_code: String,
}

impl From<sys_role::Model> for RoleDetailVo {
    fn from(r: sys_role::Model) -> Self {
        Self {
            role_id: r.id,
            role_name: r.role_name,
            role_code: r.role_code,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleVo {
    pub role_id: i64,
    pub role_name: String,
    pub role_code: String,
    pub description: String,
    pub enabled: bool,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
}

impl From<sys_role::Model> for RoleVo {
    fn from(r: sys_role::Model) -> Self {
        Self {
            role_id: r.id,
            role_name: r.role_name,
            role_code: r.role_code,
            description: r.description,
            enabled: r.enabled,
            create_time: r.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RolePermissionVo {
    pub checked_keys: Vec<i64>,
    pub half_checked_keys: Vec<i64>,
}
