use serde_json::Value;

use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};

use crate::relay::channel_router::{
    ChannelRouter, RouteSelectionExclusions, RouteSelectionPlan, RouteSelectionState,
    SelectedChannel,
};
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::token::TokenInfo;

#[derive(Clone, Copy)]
pub(crate) struct ResourceRequestSpec {
    pub endpoint_scope: &'static str,
    pub bind_resource_kind: Option<&'static str>,
    #[allow(dead_code)]
    pub delete_resource_kind: Option<&'static str>,
}

pub(crate) struct ResourceRouteState {
    exclusions: RouteSelectionExclusions,
    model_plan: Option<RouteSelectionPlan>,
    default_plan: RouteSelectionPlan,
    strict_model_routing: bool,
}

impl ResourceRouteState {
    pub(crate) async fn new(
        token_info: &TokenInfo,
        router_svc: &ChannelRouter,
        endpoint_scope: &'static str,
        requested_model: Option<&str>,
    ) -> OpenAiApiResult<Self> {
        let exclusions = RouteSelectionExclusions::default();
        let model_plan = if let Some(model) = requested_model {
            Some(
                router_svc
                    .build_channel_plan_with_exclusions(
                        &token_info.group,
                        model,
                        endpoint_scope,
                        &exclusions,
                    )
                    .await
                    .map_err(|error| {
                        OpenAiErrorResponse::internal_with("failed to build channel plan", error)
                    })?,
            )
        } else {
            None
        };
        let default_plan = router_svc
            .build_default_channel_plan_with_exclusions(
                &token_info.group,
                endpoint_scope,
                &exclusions,
            )
            .await
            .map_err(|error| {
                OpenAiErrorResponse::internal_with("failed to build default channel plan", error)
            })?;

        Ok(Self {
            exclusions,
            model_plan,
            default_plan,
            strict_model_routing: requested_model.is_some(),
        })
    }

    pub(crate) async fn select(
        &mut self,
        token_info: &TokenInfo,
        resource_affinity: &ResourceAffinityService,
        affinity_keys: &[(&'static str, String)],
        json_body: Option<&Value>,
    ) -> OpenAiApiResult<Option<SelectedChannel>> {
        for (kind, id) in resource_affinity_lookup_keys(affinity_keys, json_body) {
            if let Some(channel) = resource_affinity
                .resolve(token_info, kind, &id)
                .await
                .map_err(OpenAiErrorResponse::from)?
                && !self.exclusions.selected_is_excluded(&channel)
            {
                return Ok(Some(channel));
            }
        }

        Ok(self.select_without_affinity())
    }

    fn select_without_affinity(&mut self) -> Option<SelectedChannel> {
        if let Some(model_plan) = self.model_plan.as_mut()
            && let Some(channel) = model_plan.next()
        {
            return Some(channel);
        }

        if self.strict_model_routing {
            return None;
        }

        self.default_plan.next()
    }
}

impl RouteSelectionState for ResourceRouteState {
    fn exclude_selected_channel(&mut self, channel: &SelectedChannel) {
        self.exclusions.exclude_selected_channel(channel);
        if let Some(model_plan) = self.model_plan.as_mut() {
            model_plan.exclude_selected_channel(channel);
        }
        self.default_plan.exclude_selected_channel(channel);
    }

    fn exclude_selected_account(&mut self, channel: &SelectedChannel) {
        self.exclusions.exclude_selected_account(channel);
        if let Some(model_plan) = self.model_plan.as_mut() {
            model_plan.exclude_selected_account(channel);
        }
        self.default_plan.exclude_selected_account(channel);
    }
}

pub(crate) fn resource_affinity_lookup_keys(
    affinity_keys: &[(&'static str, String)],
    json_body: Option<&Value>,
) -> Vec<(&'static str, String)> {
    let mut keys = Vec::new();

    for (kind, id) in affinity_keys {
        if !id.trim().is_empty() {
            keys.push((*kind, id.clone()));
        }
    }

    if let Some(body) = json_body {
        for (kind, id) in referenced_resource_ids(body) {
            let exists = keys
                .iter()
                .any(|(existing_kind, existing_id)| existing_kind == &kind && existing_id == &id);
            if !exists {
                keys.push((kind, id));
            }
        }
    }

    keys
}

pub(crate) fn extract_generic_resource_id(value: &Value) -> Option<String> {
    value
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn referenced_resource_ids(body: &Value) -> Vec<(&'static str, String)> {
    let mut refs = Vec::new();
    collect_referenced_resource_ids(body, &mut refs);
    refs
}

fn collect_referenced_resource_ids(value: &Value, refs: &mut Vec<(&'static str, String)>) {
    match value {
        Value::Object(map) => {
            for (field, kind) in [
                ("response_id", "response"),
                ("previous_response_id", "response"),
                ("assistant_id", "assistant"),
                ("thread_id", "thread"),
                ("run_id", "run"),
                ("batch_id", "batch"),
                ("vector_store_id", "vector_store"),
                ("vector_store_ids", "vector_store"),
                ("file_id", "file"),
                ("file_ids", "file"),
                ("input_file_id", "file"),
                ("upload_id", "upload"),
                ("fine_tuning_job_id", "fine_tuning_job"),
            ] {
                let Some(nested) = map.get(field) else {
                    continue;
                };
                match nested {
                    Value::String(id) => push_referenced_resource_id(refs, kind, id),
                    Value::Array(items) => {
                        for item in items {
                            if let Some(id) = item.as_str() {
                                push_referenced_resource_id(refs, kind, id);
                            }
                        }
                    }
                    _ => {}
                }
            }

            for nested in map.values() {
                collect_referenced_resource_ids(nested, refs);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_referenced_resource_ids(item, refs);
            }
        }
        _ => {}
    }
}

fn push_referenced_resource_id(
    refs: &mut Vec<(&'static str, String)>,
    kind: &'static str,
    resource_id: &str,
) {
    if resource_id.trim().is_empty() {
        return;
    }

    if refs
        .iter()
        .any(|(existing_kind, existing_id)| existing_kind == &kind && existing_id == resource_id)
    {
        return;
    }

    refs.push((kind, resource_id.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_channel(channel_id: i64, account_id: i64) -> SelectedChannel {
        SelectedChannel {
            channel_id,
            channel_name: format!("channel-{channel_id}"),
            channel_type: 1,
            base_url: format!("https://{channel_id}.example.com"),
            model_mapping: serde_json::json!({}),
            api_key: format!("sk-{channel_id}"),
            account_id,
            account_name: format!("account-{account_id}"),
        }
    }

    #[test]
    fn select_does_not_fall_back_to_default_plan_when_model_was_requested() {
        let mut state = ResourceRouteState {
            exclusions: RouteSelectionExclusions::default(),
            model_plan: Some(RouteSelectionPlan::new(
                Vec::new(),
                RouteSelectionExclusions::default(),
            )),
            default_plan: RouteSelectionPlan::new(
                vec![sample_channel(22, 202)],
                RouteSelectionExclusions::default(),
            ),
            strict_model_routing: true,
        };

        let selected = state.select_without_affinity();

        assert!(selected.is_none());
    }

    #[test]
    fn select_uses_default_plan_when_model_was_not_requested() {
        let mut state = ResourceRouteState {
            exclusions: RouteSelectionExclusions::default(),
            model_plan: None,
            default_plan: RouteSelectionPlan::new(
                vec![sample_channel(22, 202)],
                RouteSelectionExclusions::default(),
            ),
            strict_model_routing: false,
        };

        let selected = state
            .select_without_affinity()
            .expect("expected default plan channel");

        assert_eq!(selected.channel_id, 22);
    }
}
