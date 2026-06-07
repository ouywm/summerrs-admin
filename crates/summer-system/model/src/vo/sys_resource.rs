//! 后端 API 资源 VO

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::{sys_menu, sys_resource};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceVo {
    pub id: i64,
    pub resource_name: String,
    pub resource_code: String,
    pub method: sys_resource::ResourceMethod,
    pub path: String,
    pub description: String,
    pub enabled: bool,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl From<sys_resource::Model> for ResourceVo {
    fn from(model: sys_resource::Model) -> Self {
        Self {
            id: model.id,
            resource_name: model.resource_name,
            resource_code: model.resource_code,
            method: model.method,
            path: model.path,
            description: model.description,
            enabled: model.enabled,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceOptionVo {
    pub id: i64,
    pub resource_name: String,
    pub resource_code: String,
    pub method: sys_resource::ResourceMethod,
    pub path: String,
    pub enabled: bool,
}

impl From<sys_resource::Model> for ResourceOptionVo {
    fn from(model: sys_resource::Model) -> Self {
        Self {
            id: model.id,
            resource_name: model.resource_name,
            resource_code: model.resource_code,
            method: model.method,
            path: model.path,
            enabled: model.enabled,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActionResourceVo {
    pub action_menu_id: i64,
    pub action_title: String,
    pub auth_mark: String,
    pub resources: Vec<ResourceOptionVo>,
}

impl ActionResourceVo {
    pub fn from_action(action: sys_menu::Model, resources: Vec<ResourceOptionVo>) -> Self {
        Self {
            action_menu_id: action.id,
            action_title: action.title,
            auth_mark: action.auth_mark,
            resources,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceActionVo {
    pub resource_id: i64,
    pub resource_name: String,
    pub resource_code: String,
    pub method: sys_resource::ResourceMethod,
    pub path: String,
    pub actions: Vec<ActionOptionVo>,
}

impl ResourceActionVo {
    pub fn from_resource(resource: sys_resource::Model, actions: Vec<ActionOptionVo>) -> Self {
        Self {
            resource_id: resource.id,
            resource_name: resource.resource_name,
            resource_code: resource.resource_code,
            method: resource.method,
            path: resource.path,
            actions,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActionOptionVo {
    pub id: i64,
    pub title: String,
    pub auth_mark: String,
    pub enabled: bool,
}

impl From<sys_menu::Model> for ActionOptionVo {
    fn from(model: sys_menu::Model) -> Self {
        Self {
            id: model.id,
            title: model.title,
            auth_mark: model.auth_mark,
            enabled: model.enabled,
        }
    }
}
