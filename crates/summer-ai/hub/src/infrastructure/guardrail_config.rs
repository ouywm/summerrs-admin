use anyhow::Context;
use sea_orm::EntityTrait;
use summer::plugin::Service;
use summer_ai_model::entity::guardrail_config;
use summer_sea_orm::DbConn;

use crate::domain::guardrail_config::{
    DomainResult, GuardrailConfigAggregate, GuardrailConfigReadRepository,
};

#[derive(Clone, Service)]
pub struct SeaOrmGuardrailConfigReadRepository {
    #[inject(component)]
    db: DbConn,
}

#[summer::async_trait]
impl GuardrailConfigReadRepository for SeaOrmGuardrailConfigReadRepository {
    async fn find_by_id(&self, id: i64) -> DomainResult<Option<GuardrailConfigAggregate>> {
        let model = guardrail_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .with_context(|| format!("query guardrail config by id failed: {id}"))?;

        Ok(model.map(map_guardrail_config_aggregate))
    }
}

fn map_guardrail_config_aggregate(model: guardrail_config::Model) -> GuardrailConfigAggregate {
    GuardrailConfigAggregate {
        id: model.id,
        scope_type: model.scope_type,
        organization_id: model.organization_id,
        project_id: model.project_id,
        enabled: model.enabled,
        mode: model.mode,
        system_rules: model.system_rules,
        allowed_file_types: model.allowed_file_types,
        max_file_size_mb: model.max_file_size_mb,
        pii_action: model.pii_action,
        secret_action: model.secret_action,
        metadata: model.metadata,
        remark: model.remark,
        create_time: model.create_time,
        update_time: model.update_time,
    }
}
