use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use summer_common::serde_utils::datetime_format;

use crate::entity::{sys_tenant, sys_tenant_datasource, sys_tenant_membership};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantDatasourceVo {
    pub id: i64,
    pub tenant_id: String,
    pub isolation_level: sys_tenant_datasource::TenantIsolationLevel,
    pub status: sys_tenant_datasource::TenantDatasourceStatus,
    pub schema_name: Option<String>,
    pub datasource_name: Option<String>,
    pub db_uri: Option<String>,
    pub db_enable_logging: Option<bool>,
    pub db_min_conns: Option<i32>,
    pub db_max_conns: Option<i32>,
    pub db_connect_timeout_ms: Option<i64>,
    pub db_idle_timeout_ms: Option<i64>,
    pub db_acquire_timeout_ms: Option<i64>,
    pub db_test_before_acquire: Option<bool>,
    pub readonly_config: Value,
    pub extra_config: Value,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub last_sync_time: Option<NaiveDateTime>,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl From<sys_tenant_datasource::Model> for TenantDatasourceVo {
    fn from(model: sys_tenant_datasource::Model) -> Self {
        Self {
            id: model.id,
            tenant_id: model.tenant_id,
            isolation_level: model.isolation_level,
            status: model.status,
            schema_name: model.schema_name,
            datasource_name: model.datasource_name,
            db_uri: model.db_uri,
            db_enable_logging: model.db_enable_logging,
            db_min_conns: model.db_min_conns,
            db_max_conns: model.db_max_conns,
            db_connect_timeout_ms: model.db_connect_timeout_ms,
            db_idle_timeout_ms: model.db_idle_timeout_ms,
            db_acquire_timeout_ms: model.db_acquire_timeout_ms,
            db_test_before_acquire: model.db_test_before_acquire,
            readonly_config: model.readonly_config,
            extra_config: model.extra_config,
            last_sync_time: model.last_sync_time,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantVo {
    pub id: i64,
    pub tenant_id: String,
    pub tenant_name: String,
    pub default_isolation_level: sys_tenant::TenantIsolationLevel,
    pub contact_name: String,
    pub contact_email: String,
    pub contact_phone: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expire_time: Option<NaiveDateTime>,
    pub status: sys_tenant::TenantStatus,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
    pub datasource: Option<TenantDatasourceVo>,
    pub member_count: u64,
}

impl TenantVo {
    pub fn from_model(
        model: sys_tenant::Model,
        datasource: Option<sys_tenant_datasource::Model>,
        member_count: u64,
    ) -> Self {
        Self {
            id: model.id,
            tenant_id: model.tenant_id,
            tenant_name: model.tenant_name,
            default_isolation_level: model.default_isolation_level,
            contact_name: model.contact_name,
            contact_email: model.contact_email,
            contact_phone: model.contact_phone,
            expire_time: model.expire_time,
            status: model.status,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
            datasource: datasource.map(TenantDatasourceVo::from),
            member_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantDetailVo {
    #[serde(flatten)]
    pub tenant: TenantVo,
    pub config: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantMembershipVo {
    pub id: i64,
    pub tenant_id: String,
    pub user_id: i64,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub role_code: String,
    pub is_default: bool,
    pub status: sys_tenant_membership::TenantMembershipStatus,
    pub source: sys_tenant_membership::TenantMembershipSource,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub joined_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub last_access_time: Option<NaiveDateTime>,
    pub remark: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantRuntimeDatasourceVo {
    pub datasource: String,
    pub reachable: bool,
    pub error: Option<String>,
    pub latency_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantRouteStateVo {
    pub rule_name: String,
    pub configured_primary: String,
    pub effective_write_target: Option<String>,
    pub healthy_replicas: Vec<String>,
    pub unhealthy: Vec<String>,
    pub failover_active: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantRuntimeRefreshVo {
    pub tenant_metadata_count: usize,
    pub datasource_count: usize,
    pub route_state_count: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantProvisionResultVo {
    pub tenant_id: String,
    pub isolation_level: sys_tenant::TenantIsolationLevel,
    pub resource_sql: Vec<String>,
    pub datasource: TenantDatasourceVo,
}
