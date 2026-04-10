use chrono::NaiveDateTime;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::Validate;

use crate::entity::{sys_tenant, sys_tenant_datasource, sys_tenant_membership};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateTenantDto {
    #[validate(length(min = 1, max = 64, message = "租户标识长度必须在1-64之间"))]
    pub tenant_id: String,
    #[validate(length(min = 1, max = 128, message = "租户名称长度必须在1-128之间"))]
    pub tenant_name: String,
    pub default_isolation_level: Option<sys_tenant::TenantIsolationLevel>,
    #[validate(length(max = 64, message = "联系人长度不能超过64"))]
    pub contact_name: Option<String>,
    #[validate(email(message = "联系人邮箱格式不正确"))]
    pub contact_email: Option<String>,
    #[validate(length(max = 32, message = "联系人手机号长度不能超过32"))]
    pub contact_phone: Option<String>,
    pub expire_time: Option<NaiveDateTime>,
    pub status: Option<sys_tenant::TenantStatus>,
    pub config: Option<Value>,
    pub metadata: Option<Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateTenantDto {
    pub fn into_active_model(self, operator: String) -> sys_tenant::ActiveModel {
        sys_tenant::ActiveModel {
            tenant_id: Set(self.tenant_id),
            tenant_name: Set(self.tenant_name),
            default_isolation_level: Set(self
                .default_isolation_level
                .unwrap_or(sys_tenant::TenantIsolationLevel::SharedRow)),
            contact_name: Set(self.contact_name.unwrap_or_default()),
            contact_email: Set(self.contact_email.unwrap_or_default()),
            contact_phone: Set(self.contact_phone.unwrap_or_default()),
            expire_time: Set(self.expire_time),
            status: Set(self.status.unwrap_or(sys_tenant::TenantStatus::Enabled)),
            config: Set(self.config.unwrap_or_else(|| serde_json::json!({}))),
            metadata: Set(self.metadata.unwrap_or_else(|| serde_json::json!({}))),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            update_by: Set(operator),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTenantDto {
    #[validate(length(min = 1, max = 128, message = "租户名称长度必须在1-128之间"))]
    pub tenant_name: Option<String>,
    pub default_isolation_level: Option<sys_tenant::TenantIsolationLevel>,
    #[validate(length(max = 64, message = "联系人长度不能超过64"))]
    pub contact_name: Option<String>,
    #[validate(email(message = "联系人邮箱格式不正确"))]
    pub contact_email: Option<String>,
    #[validate(length(max = 32, message = "联系人手机号长度不能超过32"))]
    pub contact_phone: Option<String>,
    pub expire_time: Option<Option<NaiveDateTime>>,
    pub status: Option<sys_tenant::TenantStatus>,
    pub config: Option<Value>,
    pub metadata: Option<Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateTenantDto {
    pub fn apply_to(self, active: &mut sys_tenant::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(value) = self.tenant_name {
            active.tenant_name = Set(value);
        }
        if let Some(value) = self.default_isolation_level {
            active.default_isolation_level = Set(value);
        }
        if let Some(value) = self.contact_name {
            active.contact_name = Set(value);
        }
        if let Some(value) = self.contact_email {
            active.contact_email = Set(value);
        }
        if let Some(value) = self.contact_phone {
            active.contact_phone = Set(value);
        }
        if let Some(value) = self.expire_time {
            active.expire_time = Set(value);
        }
        if let Some(value) = self.status {
            active.status = Set(value);
        }
        if let Some(value) = self.config {
            active.config = Set(value);
        }
        if let Some(value) = self.metadata {
            active.metadata = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantQueryDto {
    pub tenant_id: Option<String>,
    pub tenant_name: Option<String>,
    pub status: Option<sys_tenant::TenantStatus>,
    pub default_isolation_level: Option<sys_tenant::TenantIsolationLevel>,
    pub expire_time_start: Option<NaiveDateTime>,
    pub expire_time_end: Option<NaiveDateTime>,
    pub create_time_start: Option<NaiveDateTime>,
    pub create_time_end: Option<NaiveDateTime>,
}

impl From<TenantQueryDto> for Condition {
    fn from(query: TenantQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(value) = query.tenant_id {
            cond = cond.add(sys_tenant::Column::TenantId.contains(value));
        }
        if let Some(value) = query.tenant_name {
            cond = cond.add(sys_tenant::Column::TenantName.contains(value));
        }
        if let Some(value) = query.status {
            cond = cond.add(sys_tenant::Column::Status.eq(value));
        }
        if let Some(value) = query.default_isolation_level {
            cond = cond.add(sys_tenant::Column::DefaultIsolationLevel.eq(value));
        }
        if let Some(value) = query.expire_time_start {
            cond = cond.add(sys_tenant::Column::ExpireTime.gte(value));
        }
        if let Some(value) = query.expire_time_end {
            cond = cond.add(sys_tenant::Column::ExpireTime.lte(value));
        }
        if let Some(value) = query.create_time_start {
            cond = cond.add(sys_tenant::Column::CreateTime.gte(value));
        }
        if let Some(value) = query.create_time_end {
            cond = cond.add(sys_tenant::Column::CreateTime.lte(value));
        }
        cond
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SaveTenantDatasourceDto {
    pub isolation_level: sys_tenant_datasource::TenantIsolationLevel,
    pub status: Option<sys_tenant_datasource::TenantDatasourceStatus>,
    #[validate(length(max = 128, message = "schema 名称长度不能超过128"))]
    pub schema_name: Option<String>,
    #[validate(length(max = 128, message = "数据源名称长度不能超过128"))]
    pub datasource_name: Option<String>,
    #[validate(length(max = 1024, message = "数据库连接串长度不能超过1024"))]
    pub db_uri: Option<String>,
    pub db_enable_logging: Option<bool>,
    pub db_min_conns: Option<i32>,
    pub db_max_conns: Option<i32>,
    pub db_connect_timeout_ms: Option<i64>,
    pub db_idle_timeout_ms: Option<i64>,
    pub db_acquire_timeout_ms: Option<i64>,
    pub db_test_before_acquire: Option<bool>,
    pub readonly_config: Option<Value>,
    pub extra_config: Option<Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl SaveTenantDatasourceDto {
    pub fn into_active_model(
        self,
        tenant_id: String,
        operator: String,
    ) -> sys_tenant_datasource::ActiveModel {
        sys_tenant_datasource::ActiveModel {
            tenant_id: Set(tenant_id),
            isolation_level: Set(self.isolation_level),
            status: Set(self
                .status
                .unwrap_or(sys_tenant_datasource::TenantDatasourceStatus::Active)),
            schema_name: Set(self.schema_name),
            datasource_name: Set(self.datasource_name),
            db_uri: Set(self.db_uri),
            db_enable_logging: Set(self.db_enable_logging),
            db_min_conns: Set(self.db_min_conns),
            db_max_conns: Set(self.db_max_conns),
            db_connect_timeout_ms: Set(self.db_connect_timeout_ms),
            db_idle_timeout_ms: Set(self.db_idle_timeout_ms),
            db_acquire_timeout_ms: Set(self.db_acquire_timeout_ms),
            db_test_before_acquire: Set(self.db_test_before_acquire),
            readonly_config: Set(self
                .readonly_config
                .unwrap_or_else(|| serde_json::json!({}))),
            extra_config: Set(self.extra_config.unwrap_or_else(|| serde_json::json!({}))),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            update_by: Set(operator),
            ..Default::default()
        }
    }

    pub fn apply_to(self, active: &mut sys_tenant_datasource::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        active.isolation_level = Set(self.isolation_level);
        if let Some(value) = self.status {
            active.status = Set(value);
        }
        active.schema_name = Set(self.schema_name);
        active.datasource_name = Set(self.datasource_name);
        active.db_uri = Set(self.db_uri);
        active.db_enable_logging = Set(self.db_enable_logging);
        active.db_min_conns = Set(self.db_min_conns);
        active.db_max_conns = Set(self.db_max_conns);
        active.db_connect_timeout_ms = Set(self.db_connect_timeout_ms);
        active.db_idle_timeout_ms = Set(self.db_idle_timeout_ms);
        active.db_acquire_timeout_ms = Set(self.db_acquire_timeout_ms);
        active.db_test_before_acquire = Set(self.db_test_before_acquire);
        if let Some(value) = self.readonly_config {
            active.readonly_config = Set(value);
        }
        if let Some(value) = self.extra_config {
            active.extra_config = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangeTenantStatusDto {
    pub status: sys_tenant::TenantStatus,
    pub datasource_status: Option<sys_tenant_datasource::TenantDatasourceStatus>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionTenantDto {
    pub isolation_level: Option<sys_tenant::TenantIsolationLevel>,
    #[validate(length(max = 128, message = "schema 名称长度不能超过128"))]
    pub schema_name: Option<String>,
    #[validate(length(max = 128, message = "数据源名称长度不能超过128"))]
    pub datasource_name: Option<String>,
    #[validate(length(max = 1024, message = "数据库连接串长度不能超过1024"))]
    pub db_uri: Option<String>,
    pub db_enable_logging: Option<bool>,
    pub db_min_conns: Option<i32>,
    pub db_max_conns: Option<i32>,
    pub db_connect_timeout_ms: Option<i64>,
    pub db_idle_timeout_ms: Option<i64>,
    pub db_acquire_timeout_ms: Option<i64>,
    pub db_test_before_acquire: Option<bool>,
    #[serde(default)]
    pub base_tables: Vec<String>,
    pub readonly_config: Option<Value>,
    pub extra_config: Option<Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SaveTenantMembershipDto {
    pub user_id: i64,
    #[validate(length(max = 64, message = "租户角色编码长度不能超过64"))]
    pub role_code: Option<String>,
    pub is_default: Option<bool>,
    pub status: Option<sys_tenant_membership::TenantMembershipStatus>,
    pub source: Option<sys_tenant_membership::TenantMembershipSource>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl SaveTenantMembershipDto {
    pub fn into_active_model(
        self,
        tenant_id: String,
        operator: String,
    ) -> sys_tenant_membership::ActiveModel {
        sys_tenant_membership::ActiveModel {
            tenant_id: Set(tenant_id),
            user_id: Set(self.user_id),
            role_code: Set(self.role_code.unwrap_or_default()),
            is_default: Set(self.is_default.unwrap_or(false)),
            status: Set(self
                .status
                .unwrap_or(sys_tenant_membership::TenantMembershipStatus::Enabled)),
            source: Set(self
                .source
                .unwrap_or(sys_tenant_membership::TenantMembershipSource::Manual)),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            update_by: Set(operator),
            ..Default::default()
        }
    }

    pub fn apply_to(self, active: &mut sys_tenant_membership::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(value) = self.role_code {
            active.role_code = Set(value);
        }
        if let Some(value) = self.is_default {
            active.is_default = Set(value);
        }
        if let Some(value) = self.status {
            active.status = Set(value);
        }
        if let Some(value) = self.source {
            active.source = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue;

    use super::SaveTenantDatasourceDto;
    use crate::entity::sys_tenant_datasource;

    #[test]
    fn save_tenant_datasource_dto_maps_runtime_pool_fields_into_active_model() {
        let active = SaveTenantDatasourceDto {
            isolation_level: sys_tenant_datasource::TenantIsolationLevel::SeparateDatabase,
            status: Some(sys_tenant_datasource::TenantDatasourceStatus::Active),
            schema_name: Some("tenant_t1".to_string()),
            datasource_name: Some("tenant_ds_t1".to_string()),
            db_uri: Some("postgres://tenant-db".to_string()),
            db_enable_logging: Some(true),
            db_min_conns: Some(2),
            db_max_conns: Some(16),
            db_connect_timeout_ms: Some(1_500),
            db_idle_timeout_ms: Some(2_500),
            db_acquire_timeout_ms: Some(3_500),
            db_test_before_acquire: Some(false),
            readonly_config: None,
            extra_config: None,
            remark: None,
        }
        .into_active_model("T-1".to_string(), "tester".to_string());

        assert_eq!(active.db_enable_logging, ActiveValue::Set(Some(true)));
        assert_eq!(active.db_min_conns, ActiveValue::Set(Some(2)));
        assert_eq!(active.db_max_conns, ActiveValue::Set(Some(16)));
        assert_eq!(active.db_connect_timeout_ms, ActiveValue::Set(Some(1_500)));
        assert_eq!(active.db_idle_timeout_ms, ActiveValue::Set(Some(2_500)));
        assert_eq!(active.db_acquire_timeout_ms, ActiveValue::Set(Some(3_500)));
        assert_eq!(active.db_test_before_acquire, ActiveValue::Set(Some(false)));
    }
}
