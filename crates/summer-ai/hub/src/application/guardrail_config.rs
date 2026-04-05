use chrono::{DateTime, FixedOffset};
use std::error::Error;
use std::fmt::{Display, Formatter};

use summer::plugin::Service;

use crate::domain::guardrail_config::{GuardrailConfigAggregate, GuardrailConfigReadRepository};
use crate::infrastructure::guardrail_config::SeaOrmGuardrailConfigReadRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGuardrailConfigDetailQuery {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardrailConfigDetailDto {
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

impl From<GuardrailConfigAggregate> for GuardrailConfigDetailDto {
    fn from(value: GuardrailConfigAggregate) -> Self {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetGuardrailConfigDetailError {
    NotFound(i64),
    Unexpected(String),
}

impl Display for GetGuardrailConfigDetailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "guardrail config not found: {id}"),
            Self::Unexpected(message) => write!(f, "{message}"),
        }
    }
}

impl Error for GetGuardrailConfigDetailError {}

pub struct GetGuardrailConfigDetailUseCase<R> {
    repository: R,
}

impl<R> GetGuardrailConfigDetailUseCase<R>
where
    R: GuardrailConfigReadRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        query: GetGuardrailConfigDetailQuery,
    ) -> Result<GuardrailConfigDetailDto, GetGuardrailConfigDetailError> {
        let aggregate = self
            .repository
            .find_by_id(query.id)
            .await
            .map_err(|err| GetGuardrailConfigDetailError::Unexpected(err.to_string()))?
            .ok_or(GetGuardrailConfigDetailError::NotFound(query.id))?;

        Ok(aggregate.into())
    }
}

#[derive(Clone, Service)]
pub struct GuardrailConfigApplicationService {
    #[inject(component)]
    repository: SeaOrmGuardrailConfigReadRepository,
}

impl GuardrailConfigApplicationService {
    pub async fn detail(
        &self,
        id: i64,
    ) -> Result<GuardrailConfigDetailDto, GetGuardrailConfigDetailError> {
        GetGuardrailConfigDetailUseCase::new(self.repository.clone())
            .execute(GetGuardrailConfigDetailQuery { id })
            .await
    }
}
