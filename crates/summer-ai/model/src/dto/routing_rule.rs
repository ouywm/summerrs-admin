use crate::entity::routing::routing_rule::{self, RoutingRuleStatus};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutingRuleDto {
    pub organization_id: i64,
    pub project_id: i64,
    #[validate(length(min = 1, max = 64, message = "规则编码长度必须在1-64之间"))]
    pub rule_code: String,
    #[validate(length(min = 1, max = 128, message = "规则名称长度必须在1-128之间"))]
    pub rule_name: String,
    pub priority: Option<i32>,
    #[validate(length(min = 1, max = 32, message = "匹配类型长度必须在1-32之间"))]
    pub match_type: String,
    pub match_conditions: Option<serde_json::Value>,
    #[validate(length(min = 1, max = 32, message = "路由策略长度必须在1-32之间"))]
    pub route_strategy: String,
    #[validate(length(max = 32, message = "回退策略长度不能超过32"))]
    pub fallback_strategy: Option<String>,
    pub status: Option<RoutingRuleStatus>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub metadata: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateRoutingRuleDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_tenant_scope(self.organization_id, self.project_id)?;
        validate_schedule_window(self.start_time.as_deref(), self.end_time.as_deref())?;
        Ok(())
    }

    pub fn into_active_model(self, operator: &str) -> Result<routing_rule::ActiveModel, String> {
        let (start_time, end_time) = parse_schedule_window(self.start_time, self.end_time)?;
        Ok(routing_rule::ActiveModel {
            organization_id: Set(self.organization_id),
            project_id: Set(self.project_id),
            rule_code: Set(self.rule_code),
            rule_name: Set(self.rule_name),
            priority: Set(self.priority.unwrap_or(0)),
            match_type: Set(self.match_type),
            match_conditions: Set(self
                .match_conditions
                .unwrap_or_else(|| serde_json::json!({}))),
            route_strategy: Set(self.route_strategy),
            fallback_strategy: Set(self.fallback_strategy.unwrap_or_else(|| "none".to_string())),
            status: Set(self.status.unwrap_or(RoutingRuleStatus::Enabled)),
            start_time: Set(start_time),
            end_time: Set(end_time),
            metadata: Set(self.metadata.unwrap_or_else(|| serde_json::json!({}))),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoutingRuleDto {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "规则编码长度必须在1-64之间"))]
    pub rule_code: Option<String>,
    #[validate(length(min = 1, max = 128, message = "规则名称长度必须在1-128之间"))]
    pub rule_name: Option<String>,
    pub priority: Option<i32>,
    #[validate(length(min = 1, max = 32, message = "匹配类型长度必须在1-32之间"))]
    pub match_type: Option<String>,
    pub match_conditions: Option<serde_json::Value>,
    #[validate(length(min = 1, max = 32, message = "路由策略长度必须在1-32之间"))]
    pub route_strategy: Option<String>,
    #[validate(length(max = 32, message = "回退策略长度不能超过32"))]
    pub fallback_strategy: Option<String>,
    pub status: Option<RoutingRuleStatus>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub metadata: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateRoutingRuleDto {
    pub fn validate_business_rules(&self, current: &routing_rule::Model) -> Result<(), String> {
        let merged_start_time = self.start_time.clone().or_else(|| {
            current
                .start_time
                .as_ref()
                .map(chrono::DateTime::to_rfc3339)
        });
        let merged_end_time = self
            .end_time
            .clone()
            .or_else(|| current.end_time.as_ref().map(chrono::DateTime::to_rfc3339));
        validate_tenant_scope(
            self.organization_id.unwrap_or(current.organization_id),
            self.project_id.unwrap_or(current.project_id),
        )?;
        validate_schedule_window(merged_start_time.as_deref(), merged_end_time.as_deref())?;
        Ok(())
    }

    pub fn apply_to(
        self,
        active: &mut routing_rule::ActiveModel,
        operator: &str,
    ) -> Result<(), String> {
        active.update_by = Set(operator.to_string());
        if let Some(v) = self.organization_id {
            active.organization_id = Set(v);
        }
        if let Some(v) = self.project_id {
            active.project_id = Set(v);
        }
        if let Some(v) = self.rule_code {
            active.rule_code = Set(v);
        }
        if let Some(v) = self.rule_name {
            active.rule_name = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.match_type {
            active.match_type = Set(v);
        }
        if let Some(v) = self.match_conditions {
            active.match_conditions = Set(v);
        }
        if let Some(v) = self.route_strategy {
            active.route_strategy = Set(v);
        }
        if let Some(v) = self.fallback_strategy {
            active.fallback_strategy = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.start_time {
            active.start_time = Set(Some(parse_datetime(&v)?));
        }
        if let Some(v) = self.end_time {
            active.end_time = Set(Some(parse_datetime(&v)?));
        }
        if let Some(v) = self.metadata {
            active.metadata = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRuleQueryDto {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub status: Option<RoutingRuleStatus>,
    pub rule_code: Option<String>,
    pub keyword: Option<String>,
}

impl From<RoutingRuleQueryDto> for Condition {
    fn from(query: RoutingRuleQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.organization_id {
            cond = cond.add(routing_rule::Column::OrganizationId.eq(v));
        }
        if let Some(v) = query.project_id {
            cond = cond.add(routing_rule::Column::ProjectId.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(routing_rule::Column::Status.eq(v));
        }
        if let Some(v) = query.rule_code {
            cond = cond.add(routing_rule::Column::RuleCode.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(routing_rule::Column::RuleCode.contains(&keyword))
                        .add(routing_rule::Column::RuleName.contains(&keyword))
                        .add(routing_rule::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn validate_tenant_scope(organization_id: i64, project_id: i64) -> Result<(), String> {
    if organization_id < 0 || project_id < 0 {
        return Err("organizationId / projectId 不能为负数".to_string());
    }
    Ok(())
}

fn validate_schedule_window(
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<(), String> {
    let start = start_time.map(parse_datetime_str).transpose()?;
    let end = end_time.map(parse_datetime_str).transpose()?;
    if let (Some(start), Some(end)) = (start, end)
        && start > end
    {
        return Err("startTime 不能晚于 endTime".to_string());
    }
    Ok(())
}

fn parse_schedule_window(
    start_time: Option<String>,
    end_time: Option<String>,
) -> Result<
    (
        Option<sea_orm::prelude::DateTimeWithTimeZone>,
        Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ),
    String,
> {
    let start = start_time.map(|v| parse_datetime(&v)).transpose()?;
    let end = end_time.map(|v| parse_datetime(&v)).transpose()?;
    if let (Some(start), Some(end)) = (&start, &end)
        && start > end
    {
        return Err("startTime 不能晚于 endTime".to_string());
    }
    Ok((start, end))
}

fn parse_datetime(value: &str) -> Result<sea_orm::prelude::DateTimeWithTimeZone, String> {
    parse_datetime_str(value)
}

fn parse_datetime_str(value: &str) -> Result<sea_orm::prelude::DateTimeWithTimeZone, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| "时间字段必须是 RFC3339 时间".to_string())
}
