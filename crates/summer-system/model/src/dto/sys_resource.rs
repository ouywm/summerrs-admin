//! 后端 API 资源 DTO

use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_resource;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateResourceDto {
    #[validate(length(min = 1, max = 128, message = "资源名称长度必须在1-128之间"))]
    pub resource_name: String,
    #[validate(length(min = 1, max = 128, message = "资源编码长度必须在1-128之间"))]
    pub resource_code: String,
    pub method: sys_resource::ResourceMethod,
    #[validate(length(min = 1, max = 256, message = "API路径长度必须在1-256之间"))]
    pub path: String,
    #[validate(length(max = 512, message = "资源说明长度不能超过512"))]
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

impl From<CreateResourceDto> for sys_resource::ActiveModel {
    fn from(dto: CreateResourceDto) -> Self {
        Self {
            id: NotSet,
            resource_name: Set(dto.resource_name),
            resource_code: Set(dto.resource_code),
            method: Set(dto.method),
            path: Set(dto.path),
            description: Set(dto.description.unwrap_or_default()),
            enabled: Set(dto.enabled.unwrap_or(true)),
            create_time: NotSet,
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResourceDto {
    #[validate(length(min = 1, max = 128, message = "资源名称长度必须在1-128之间"))]
    pub resource_name: Option<String>,
    #[validate(length(min = 1, max = 128, message = "资源编码长度必须在1-128之间"))]
    pub resource_code: Option<String>,
    pub method: Option<sys_resource::ResourceMethod>,
    #[validate(length(min = 1, max = 256, message = "API路径长度必须在1-256之间"))]
    pub path: Option<String>,
    #[validate(length(max = 512, message = "资源说明长度不能超过512"))]
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

impl UpdateResourceDto {
    pub fn apply_to(self, active: &mut sys_resource::ActiveModel) {
        if let Some(resource_name) = self.resource_name {
            active.resource_name = Set(resource_name);
        }
        if let Some(resource_code) = self.resource_code {
            active.resource_code = Set(resource_code);
        }
        if let Some(method) = self.method {
            active.method = Set(method);
        }
        if let Some(path) = self.path {
            active.path = Set(path);
        }
        if let Some(description) = self.description {
            active.description = Set(description);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResourceEnabledDto {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceQueryDto {
    pub id: Option<i64>,
    pub resource_name: Option<String>,
    pub resource_code: Option<String>,
    pub method: Option<sys_resource::ResourceMethod>,
    pub path: Option<String>,
    pub enabled: Option<bool>,
}

impl From<ResourceQueryDto> for Condition {
    fn from(query: ResourceQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(id) = query.id {
            cond = cond.add(sys_resource::Column::Id.eq(id));
        }
        if let Some(resource_name) = query.resource_name {
            cond = cond.add(sys_resource::Column::ResourceName.contains(resource_name));
        }
        if let Some(resource_code) = query.resource_code {
            cond = cond.add(sys_resource::Column::ResourceCode.contains(resource_code));
        }
        if let Some(method) = query.method {
            cond = cond.add(sys_resource::Column::Method.eq(method));
        }
        if let Some(path) = query.path {
            cond = cond.add(sys_resource::Column::Path.contains(path));
        }
        if let Some(enabled) = query.enabled {
            cond = cond.add(sys_resource::Column::Enabled.eq(enabled));
        }
        cond
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SaveActionResourcesDto {
    pub resource_ids: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue;

    use super::*;

    #[test]
    fn create_resource_defaults_enabled_and_description() {
        let active: sys_resource::ActiveModel = CreateResourceDto {
            resource_name: "用户列表".to_string(),
            resource_code: "system:user:list.query".to_string(),
            method: sys_resource::ResourceMethod::Get,
            path: "/api/user/list".to_string(),
            description: None,
            enabled: None,
        }
        .into();

        assert_eq!(active.description, ActiveValue::Set(String::new()));
        assert_eq!(active.enabled, ActiveValue::Set(true));
    }

    #[test]
    fn update_resource_applies_only_present_fields() {
        let mut active = sys_resource::ActiveModel {
            resource_name: Set("旧名称".to_string()),
            resource_code: Set("old.code".to_string()),
            method: Set(sys_resource::ResourceMethod::Get),
            path: Set("/api/old".to_string()),
            enabled: Set(true),
            ..Default::default()
        };

        UpdateResourceDto {
            resource_name: Some("新名称".to_string()),
            resource_code: None,
            method: Some(sys_resource::ResourceMethod::Post),
            path: None,
            description: Some("说明".to_string()),
            enabled: Some(false),
        }
        .apply_to(&mut active);

        assert_eq!(active.resource_name, ActiveValue::Set("新名称".to_string()));
        assert_eq!(
            active.resource_code,
            ActiveValue::Set("old.code".to_string())
        );
        assert_eq!(
            active.method,
            ActiveValue::Set(sys_resource::ResourceMethod::Post)
        );
        assert_eq!(active.path, ActiveValue::Set("/api/old".to_string()));
        assert_eq!(active.description, ActiveValue::Set("说明".to_string()));
        assert_eq!(active.enabled, ActiveValue::Set(false));
    }
}
