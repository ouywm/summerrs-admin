use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::extractor::Path;
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::get_api;

use crate::application::guardrail_config::{
    GetGuardrailConfigDetailError, GuardrailConfigApplicationService, GuardrailConfigDetailDto,
};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailConfigDetailResponse {
    pub id: i64,
    pub scope_type: String,
    pub organization_id: i64,
    pub project_id: i64,
    pub enabled: bool,
    pub mode: String,
    pub system_rules: serde_json::Value,
    pub allowed_file_types: serde_json::Value,
    pub max_file_size_mb: i32,
    pub pii_action: String,
    pub secret_action: String,
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl From<GuardrailConfigDetailDto> for GuardrailConfigDetailResponse {
    fn from(value: GuardrailConfigDetailDto) -> Self {
        Self {
            id: value.id,
            scope_type: value.scope_type,
            organization_id: value.organization_id,
            project_id: value.project_id,
            enabled: value.enabled,
            mode: value.mode,
            system_rules: value.system_rules,
            allowed_file_types: value.allowed_file_types,
            max_file_size_mb: value.max_file_size_mb,
            pii_action: value.pii_action,
            secret_action: value.secret_action,
            metadata: value.metadata,
            remark: value.remark,
            create_time: value.create_time,
            update_time: value.update_time,
        }
    }
}

#[get_api("/ddd-sample/guardrail-config/{id}")]
pub async fn detail(
    Component(service): Component<GuardrailConfigApplicationService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GuardrailConfigDetailResponse>> {
    let detail = service
        .detail(id)
        .await
        .map(GuardrailConfigDetailResponse::from)
        .map_err(map_detail_error)?;

    Ok(Json(detail))
}

fn map_detail_error(error: GetGuardrailConfigDetailError) -> ApiErrors {
    match error {
        GetGuardrailConfigDetailError::NotFound(_) => {
            ApiErrors::NotFound("Guardrail 配置不存在".to_string())
        }
        GetGuardrailConfigDetailError::Unexpected(message) => {
            ApiErrors::Internal(anyhow::anyhow!(message))
        }
    }
}
