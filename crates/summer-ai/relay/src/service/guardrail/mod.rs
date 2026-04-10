use chrono::NaiveDate;
use regex::Regex;
use summer_ai_model::entity::guardrails::guardrail_rule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailAction {
    Allow,
    Block,
    Redact,
    Warn,
    Quarantine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailHit {
    pub action: GuardrailAction,
    pub matched_pattern: String,
    pub sample_excerpt: String,
}

#[derive(Debug, Clone)]
pub struct GuardrailMetricDailyStatement {
    pub sql: String,
}

pub fn wildcard_match(value: &str, pattern: &str) -> bool {
    if pattern == "*" || pattern.is_empty() {
        return true;
    }

    let mut regex = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    Regex::new(&regex)
        .map(|compiled| compiled.is_match(value))
        .unwrap_or(false)
}

pub fn evaluate_rule_against_text(
    rule: &guardrail_rule::Model,
    endpoint: &str,
    model: &str,
    text: &str,
) -> Option<GuardrailHit> {
    if !rule.enabled
        || !wildcard_match(endpoint, &rule.endpoint_pattern)
        || !wildcard_match(model, &rule.model_pattern)
    {
        return None;
    }

    match rule.rule_type.as_str() {
        "blocked_terms" => {
            let terms = rule
                .rule_config
                .get("terms")
                .and_then(serde_json::Value::as_array)?;
            for term in terms.iter().filter_map(serde_json::Value::as_str) {
                if text.to_lowercase().contains(&term.to_lowercase()) {
                    return Some(GuardrailHit {
                        action: parse_action(&rule.action),
                        matched_pattern: term.to_string(),
                        sample_excerpt: text.to_string(),
                    });
                }
            }
            None
        }
        "custom_regex" => {
            let pattern = rule
                .rule_config
                .get("pattern")
                .and_then(serde_json::Value::as_str)?;
            let regex = Regex::new(pattern).ok()?;
            let matched = regex.find(text)?;
            Some(GuardrailHit {
                action: parse_action(&rule.action),
                matched_pattern: pattern.to_string(),
                sample_excerpt: matched.as_str().to_string(),
            })
        }
        _ => None,
    }
}

pub fn apply_redaction(text: &str, hit: &GuardrailHit) -> String {
    text.replacen(&hit.sample_excerpt, "[REDACTED]", 1)
}

pub fn build_metric_daily_upsert_statement(
    stats_date: NaiveDate,
    organization_id: i64,
    project_id: i64,
    rule_id: i64,
    rule_code: &str,
    action: GuardrailAction,
    passed: bool,
    latency_ms: i32,
) -> GuardrailMetricDailyStatement {
    let (blocked, redacted, warned, flagged) = match action {
        GuardrailAction::Block => (1, 0, 0, 0),
        GuardrailAction::Redact => (0, 1, 0, 0),
        GuardrailAction::Warn => (0, 0, 1, 0),
        GuardrailAction::Quarantine => (0, 0, 0, 1),
        GuardrailAction::Allow => (0, 0, 0, 0),
    };
    let passed_count = if passed { 1 } else { 0 };
    GuardrailMetricDailyStatement {
        sql: format!(
            "INSERT INTO ai.guardrail_metric_daily (stats_date, organization_id, project_id, rule_id, rule_code, requests_evaluated, passed_count, blocked_count, redacted_count, warned_count, flagged_count, avg_latency_ms) VALUES ('{stats_date}', {organization_id}, {project_id}, {rule_id}, '{rule_code}', 1, {passed_count}, {blocked}, {redacted}, {warned}, {flagged}, {latency_ms}) ON CONFLICT (stats_date, organization_id, project_id, rule_id) DO UPDATE SET requests_evaluated = ai.guardrail_metric_daily.requests_evaluated + 1, passed_count = ai.guardrail_metric_daily.passed_count + EXCLUDED.passed_count, blocked_count = ai.guardrail_metric_daily.blocked_count + EXCLUDED.blocked_count, redacted_count = ai.guardrail_metric_daily.redacted_count + EXCLUDED.redacted_count, warned_count = ai.guardrail_metric_daily.warned_count + EXCLUDED.warned_count, flagged_count = ai.guardrail_metric_daily.flagged_count + EXCLUDED.flagged_count, avg_latency_ms = EXCLUDED.avg_latency_ms"
        ),
    }
}

fn parse_action(action: &str) -> GuardrailAction {
    match action {
        "allow" => GuardrailAction::Allow,
        "block" => GuardrailAction::Block,
        "redact" => GuardrailAction::Redact,
        "warn" => GuardrailAction::Warn,
        "quarantine" => GuardrailAction::Quarantine,
        _ => GuardrailAction::Warn,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, Utc};
    use serde_json::json;
    use summer_ai_model::entity::guardrail_rule::{self, GuardrailRuleSeverity};

    use super::{
        GuardrailAction, apply_redaction, build_metric_daily_upsert_statement,
        evaluate_rule_against_text, wildcard_match,
    };

    fn sample_rule(
        rule_type: &str,
        action: &str,
        endpoint_pattern: &str,
        model_pattern: &str,
        rule_config: serde_json::Value,
    ) -> guardrail_rule::Model {
        let now = Utc::now().fixed_offset();
        guardrail_rule::Model {
            id: 11,
            guardrail_config_id: 7,
            organization_id: 0,
            project_id: 3,
            team_id: 0,
            token_id: 0,
            service_account_id: 0,
            rule_code: "block_secret".into(),
            rule_name: "Block secret".into(),
            rule_type: rule_type.into(),
            phase: "request_input".into(),
            action: action.into(),
            priority: 100,
            enabled: true,
            severity: GuardrailRuleSeverity::High,
            model_pattern: model_pattern.into(),
            endpoint_pattern: endpoint_pattern.into(),
            condition_json: json!({}),
            rule_config,
            metadata: json!({}),
            remark: String::new(),
            create_by: "tester".into(),
            create_time: now,
            update_by: "tester".into(),
            update_time: now,
        }
    }

    #[test]
    fn wildcard_match_supports_star_patterns() {
        assert!(wildcard_match("/v1/chat/completions", "/v1/chat/*"));
        assert!(wildcard_match("gpt-5.4", "gpt-*"));
        assert!(!wildcard_match("/v1/responses", "/v1/chat/*"));
    }

    #[test]
    fn blocked_terms_rule_reports_block_hit() {
        let rule = sample_rule(
            "blocked_terms",
            "block",
            "/v1/chat/*",
            "gpt-*",
            json!({
                "terms": ["secret", "apikey"]
            }),
        );

        let hit = evaluate_rule_against_text(
            &rule,
            "/v1/chat/completions",
            "gpt-5.4",
            "this contains a secret value",
        )
        .expect("hit");

        assert_eq!(hit.action, GuardrailAction::Block);
        assert_eq!(hit.matched_pattern, "secret");
        assert!(hit.sample_excerpt.contains("secret"));
    }

    #[test]
    fn custom_regex_rule_supports_redaction() {
        let rule = sample_rule(
            "custom_regex",
            "redact",
            "/v1/chat/*",
            "gpt-*",
            json!({
                "pattern": "sk-[A-Za-z0-9]+"
            }),
        );

        let hit = evaluate_rule_against_text(
            &rule,
            "/v1/chat/completions",
            "gpt-5.4",
            "token is sk-abc123",
        )
        .expect("regex hit");

        assert_eq!(hit.action, GuardrailAction::Redact);
        assert_eq!(
            apply_redaction("token is sk-abc123", &hit),
            "token is [REDACTED]"
        );
    }

    #[test]
    fn metric_daily_statement_updates_action_specific_counters() {
        let statement = build_metric_daily_upsert_statement(
            NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date"),
            0,
            3,
            11,
            "block_secret",
            GuardrailAction::Block,
            true,
            18,
        );
        let sql = &statement.sql;

        assert!(sql.contains("INSERT INTO ai.guardrail_metric_daily"));
        assert!(sql.contains("requests_evaluated"));
        assert!(sql.contains("passed_count"));
        assert!(sql.contains("blocked_count"));
        assert!(sql.contains("avg_latency_ms"));
        assert!(sql.contains("ON CONFLICT (stats_date, organization_id, project_id, rule_id)"));
    }
}
