use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde::de::Deserializer;
use summer::plugin::Service;
use summer_ai_model::entity::alerts::{alert_event, alert_rule};
use summer_common::error::ApiResult;
use summer_mail::{AsyncTransport, Mailbox, Mailer, Message};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertChannelConfig {
    #[serde(default)]
    channels: Vec<AlertChannelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertChannelEntry {
    #[serde(rename = "type")]
    channel_type: String,
    #[serde(default = "default_channel_enabled")]
    enabled: bool,
    #[serde(default)]
    url: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    from: String,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    cc: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    bcc: Vec<String>,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    message_type: WecomMessageType,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    mentioned_list: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    mentioned_mobile_list: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum WecomMessageType {
    Text,
    #[default]
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WebhookTarget {
    pub(super) url: String,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) headers: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EmailTarget {
    pub(super) from: String,
    pub(super) to: Vec<String>,
    pub(super) cc: Vec<String>,
    pub(super) bcc: Vec<String>,
    pub(super) subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WecomTarget {
    pub(super) url: String,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) headers: HashMap<String, String>,
    pub(super) message_type: WecomMessageType,
    pub(super) mentioned_list: Vec<String>,
    pub(super) mentioned_mobile_list: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WebhookDispatch {
    pub(super) url: String,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) headers: HashMap<String, String>,
    pub(super) payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EmailDispatch {
    pub(super) from: String,
    pub(super) to: Vec<String>,
    pub(super) cc: Vec<String>,
    pub(super) bcc: Vec<String>,
    pub(super) subject: String,
    pub(super) text_body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WecomDispatch {
    pub(super) url: String,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) headers: HashMap<String, String>,
    pub(super) payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AlertDispatch {
    Webhook(WebhookDispatch),
    Email(EmailDispatch),
    Wecom(WecomDispatch),
}

#[derive(Clone, Service)]
pub(super) struct AlertNotifierService {
    #[inject(component)]
    client: reqwest::Client,

    #[inject(component)]
    mailer: Mailer,
}

pub(super) fn enabled_webhook_targets(channel_config: &serde_json::Value) -> Vec<WebhookTarget> {
    parse_channel_config(channel_config)
        .channels
        .into_iter()
        .filter_map(|channel| {
            if !channel.enabled || channel.channel_type != "webhook" {
                return None;
            }

            let url = channel.url.trim().to_string();
            if url.is_empty() {
                return None;
            }

            Some(WebhookTarget {
                url,
                timeout_seconds: channel.timeout_seconds,
                headers: channel.headers,
            })
        })
        .collect()
}

pub(super) fn enabled_email_targets(channel_config: &serde_json::Value) -> Vec<EmailTarget> {
    parse_channel_config(channel_config)
        .channels
        .into_iter()
        .filter_map(|channel| {
            if !channel.enabled || channel.channel_type != "email" {
                return None;
            }

            let from = channel.from.trim().to_string();
            if !is_valid_mailbox(&from) {
                return None;
            }

            let to = filter_valid_mailboxes(channel.to);
            if to.is_empty() {
                return None;
            }

            Some(EmailTarget {
                from,
                to,
                cc: filter_valid_mailboxes(channel.cc),
                bcc: filter_valid_mailboxes(channel.bcc),
                subject: channel.subject.trim().to_string(),
            })
        })
        .collect()
}

pub(super) fn enabled_wecom_targets(channel_config: &serde_json::Value) -> Vec<WecomTarget> {
    parse_channel_config(channel_config)
        .channels
        .into_iter()
        .filter_map(|channel| {
            if !channel.enabled || channel.channel_type != "wecom" {
                return None;
            }

            let url = channel.url.trim().to_string();
            if url.is_empty() {
                return None;
            }

            Some(WecomTarget {
                url,
                timeout_seconds: channel.timeout_seconds,
                headers: channel.headers,
                message_type: channel.message_type,
                mentioned_list: channel.mentioned_list,
                mentioned_mobile_list: channel.mentioned_mobile_list,
            })
        })
        .collect()
}

pub(super) fn build_webhook_dispatches(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
) -> Vec<WebhookDispatch> {
    let payload = build_webhook_payload(rule, event);

    enabled_webhook_targets(&rule.channel_config)
        .into_iter()
        .map(|target| WebhookDispatch {
            url: target.url,
            timeout_seconds: target.timeout_seconds,
            headers: target.headers,
            payload: payload.clone(),
        })
        .collect()
}

pub(super) fn build_email_dispatches(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
) -> Vec<EmailDispatch> {
    let text_body = build_email_text_body(rule, event);

    enabled_email_targets(&rule.channel_config)
        .into_iter()
        .map(|target| EmailDispatch {
            from: target.from,
            to: target.to,
            cc: target.cc,
            bcc: target.bcc,
            subject: if target.subject.is_empty() {
                default_email_subject(rule, event)
            } else {
                target.subject
            },
            text_body: text_body.clone(),
        })
        .collect()
}

pub(super) fn build_wecom_dispatches(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
) -> Vec<WecomDispatch> {
    enabled_wecom_targets(&rule.channel_config)
        .into_iter()
        .map(|target| {
            let payload = build_wecom_payload(rule, event, &target);
            WecomDispatch {
                url: target.url,
                timeout_seconds: target.timeout_seconds,
                headers: target.headers,
                payload,
            }
        })
        .collect()
}

pub(super) fn build_notification_dispatches(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
) -> Vec<AlertDispatch> {
    let mut dispatches = Vec::new();
    dispatches.extend(
        build_webhook_dispatches(rule, event)
            .into_iter()
            .map(AlertDispatch::Webhook),
    );
    dispatches.extend(
        build_email_dispatches(rule, event)
            .into_iter()
            .map(AlertDispatch::Email),
    );
    dispatches.extend(
        build_wecom_dispatches(rule, event)
            .into_iter()
            .map(AlertDispatch::Wecom),
    );
    dispatches
}

impl AlertNotifierService {
    pub(super) async fn notify_new_event(
        &self,
        rule: &alert_rule::Model,
        event: &alert_event::Model,
    ) -> ApiResult<usize> {
        let dispatches = build_notification_dispatches(rule, event);

        for dispatch in &dispatches {
            match dispatch {
                AlertDispatch::Webhook(webhook) => self.send_webhook(webhook).await?,
                AlertDispatch::Email(email) => self.send_email(email).await?,
                AlertDispatch::Wecom(wecom) => self.send_wecom(wecom).await?,
            }
        }

        Ok(dispatches.len())
    }

    async fn send_webhook(&self, dispatch: &WebhookDispatch) -> ApiResult<()> {
        let mut request = self.client.post(&dispatch.url).json(&dispatch.payload);

        if let Some(timeout_seconds) = dispatch.timeout_seconds {
            request = request.timeout(Duration::from_secs(timeout_seconds));
        }

        for (key, value) in &dispatch.headers {
            request = request.header(key, value);
        }

        request
            .send()
            .await
            .with_context(|| format!("发送告警 webhook 失败: {}", dispatch.url))?
            .error_for_status()
            .with_context(|| format!("告警 webhook 返回非成功状态: {}", dispatch.url))?;

        Ok(())
    }

    async fn send_email(&self, dispatch: &EmailDispatch) -> ApiResult<()> {
        let mut builder = Message::builder()
            .from(parse_mailbox(&dispatch.from).context("解析告警邮件发件人失败")?)
            .subject(dispatch.subject.clone());

        for mailbox in &dispatch.to {
            builder = builder.to(parse_mailbox(mailbox).context("解析告警邮件收件人失败")?);
        }
        for mailbox in &dispatch.cc {
            builder = builder.cc(parse_mailbox(mailbox).context("解析告警邮件抄送人失败")?);
        }
        for mailbox in &dispatch.bcc {
            builder = builder.bcc(parse_mailbox(mailbox).context("解析告警邮件密送人失败")?);
        }

        let message = builder
            .body(dispatch.text_body.clone())
            .context("构建告警邮件失败")?;

        self.mailer
            .send(message)
            .await
            .context("发送告警邮件失败")?;

        Ok(())
    }

    async fn send_wecom(&self, dispatch: &WecomDispatch) -> ApiResult<()> {
        let mut request = self.client.post(&dispatch.url).json(&dispatch.payload);

        if let Some(timeout_seconds) = dispatch.timeout_seconds {
            request = request.timeout(Duration::from_secs(timeout_seconds));
        }

        for (key, value) in &dispatch.headers {
            request = request.header(key, value);
        }

        request
            .send()
            .await
            .with_context(|| format!("发送企微告警失败: {}", dispatch.url))?
            .error_for_status()
            .with_context(|| format!("企微告警返回非成功状态: {}", dispatch.url))?;

        Ok(())
    }
}

fn parse_channel_config(channel_config: &serde_json::Value) -> AlertChannelConfig {
    serde_json::from_value(channel_config.clone()).unwrap_or_default()
}

fn default_channel_enabled() -> bool {
    true
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringListValue {
        One(String),
        Many(Vec<String>),
    }

    let value = Option::<StringListValue>::deserialize(deserializer)?;
    Ok(match value {
        None => Vec::new(),
        Some(StringListValue::One(value)) => vec![value],
        Some(StringListValue::Many(values)) => values,
    }
    .into_iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    .collect())
}

fn is_valid_mailbox(value: &str) -> bool {
    parse_mailbox(value).is_ok()
}

fn filter_valid_mailboxes(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| is_valid_mailbox(value))
        .collect()
}

fn parse_mailbox(value: &str) -> anyhow::Result<Mailbox> {
    value
        .parse::<Mailbox>()
        .with_context(|| format!("邮箱地址格式无效: {value}"))
}

fn build_webhook_payload(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
) -> serde_json::Value {
    serde_json::json!({
        "rule": {
            "id": rule.id,
            "domainCode": rule.domain_code,
            "ruleCode": rule.rule_code,
            "ruleName": rule.rule_name,
            "severity": rule.severity,
            "metricKey": rule.metric_key,
        },
        "event": {
            "id": event.id,
            "eventCode": event.event_code,
            "severity": event.severity,
            "status": event.status as i16,
            "sourceDomain": event.source_domain,
            "sourceRef": event.source_ref,
            "title": event.title,
            "detail": event.detail,
            "payload": event.payload,
            "firstTriggeredAt": event.first_triggered_at,
            "lastTriggeredAt": event.last_triggered_at,
        }
    })
}

fn default_email_subject(rule: &alert_rule::Model, event: &alert_event::Model) -> String {
    format!("[AI告警][P{}] {}", rule.severity, event.title)
}

fn build_email_text_body(rule: &alert_rule::Model, event: &alert_event::Model) -> String {
    format!(
        "规则: {}\n规则编码: {}\n指标: {}\n事件编码: {}\n标题: {}\n来源: {}\n详情: {}\n载荷: {}",
        rule.rule_name,
        rule.rule_code,
        rule.metric_key,
        event.event_code,
        event.title,
        event.source_ref,
        event.detail,
        event.payload
    )
}

fn build_wecom_payload(
    rule: &alert_rule::Model,
    event: &alert_event::Model,
    target: &WecomTarget,
) -> serde_json::Value {
    let text_content = build_email_text_body(rule, event);

    if target.message_type == WecomMessageType::Text
        || !target.mentioned_list.is_empty()
        || !target.mentioned_mobile_list.is_empty()
    {
        serde_json::json!({
            "msgtype": "text",
            "text": {
                "content": text_content,
                "mentioned_list": target.mentioned_list,
                "mentioned_mobile_list": target.mentioned_mobile_list,
            }
        })
    } else {
        serde_json::json!({
            "msgtype": "markdown",
            "markdown": {
                "content": format!(
                    "## {}\n> 规则: `{}`\n> 指标: `{}`\n> 来源: `{}`\n\n{}",
                    event.title,
                    rule.rule_code,
                    rule.metric_key,
                    event.source_ref,
                    event.detail
                )
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use summer_ai_model::entity::alerts::alert_event::{self, AlertEventStatus};
    use summer_ai_model::entity::alerts::alert_rule::{self, AlertRuleStatus};

    use super::{
        AlertDispatch, WecomMessageType, build_email_dispatches, build_notification_dispatches,
        build_webhook_dispatches, build_wecom_dispatches, enabled_email_targets,
        enabled_webhook_targets, enabled_wecom_targets,
    };

    fn sample_rule(channel_config: serde_json::Value) -> alert_rule::Model {
        let now = Utc::now().fixed_offset();
        alert_rule::Model {
            id: 7,
            domain_code: "relay".into(),
            rule_code: "relay_success_rate_low".into(),
            rule_name: "渠道成功率过低".into(),
            severity: 2,
            metric_key: "success_rate".into(),
            condition_expr: String::new(),
            threshold_config: json!({
                "operator": "lt",
                "value": 90
            }),
            channel_config,
            silence_seconds: 0,
            status: AlertRuleStatus::Enabled,
            create_by: "tester".into(),
            create_time: now,
            update_by: "tester".into(),
            update_time: now,
        }
    }

    fn sample_event() -> alert_event::Model {
        let now = Utc::now().fixed_offset();
        alert_event::Model {
            id: 9,
            alert_rule_id: 7,
            event_code: "altevt_test".into(),
            severity: 2,
            status: AlertEventStatus::Open,
            source_domain: "daily_stats".into(),
            source_ref: "2026-04-09|u:11|p:22|c:33|a:44|m:gpt-5.4".into(),
            title: "渠道成功率过低 触发告警".into(),
            detail: "success_rate=82".into(),
            payload: json!({
                "metricKey": "success_rate",
                "metricValue": 82.0
            }),
            first_triggered_at: now,
            last_triggered_at: now,
            ack_by: String::new(),
            ack_time: None,
            resolved_by: String::new(),
            resolved_time: None,
            create_time: now,
        }
    }

    #[test]
    fn enabled_webhook_targets_extracts_only_enabled_webhook_channels() {
        let channel_config = json!({
            "channels": [
                {
                    "type": "webhook",
                    "enabled": true,
                    "url": "https://example.com/alerts"
                },
                {
                    "type": "email",
                    "enabled": true,
                    "to": ["ops@example.com"]
                },
                {
                    "type": "webhook",
                    "enabled": false,
                    "url": "https://example.com/disabled"
                }
            ]
        });

        let targets = enabled_webhook_targets(&channel_config);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].url, "https://example.com/alerts");
    }

    #[test]
    fn enabled_webhook_targets_skip_missing_or_blank_urls() {
        let channel_config = json!({
            "channels": [
                {
                    "type": "webhook",
                    "enabled": true
                },
                {
                    "type": "webhook",
                    "enabled": true,
                    "url": ""
                }
            ]
        });

        let targets = enabled_webhook_targets(&channel_config);

        assert!(targets.is_empty());
    }

    #[test]
    fn enabled_email_targets_extracts_only_valid_email_channels() {
        let channel_config = json!({
            "channels": [
                {
                    "type": "email",
                    "enabled": true,
                    "from": "alerts@example.com",
                    "to": ["ops@example.com", "bad-address"],
                    "cc": "audit@example.com"
                },
                {
                    "type": "email",
                    "enabled": true,
                    "from": "invalid-from",
                    "to": ["ops@example.com"]
                }
            ]
        });

        let targets = enabled_email_targets(&channel_config);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].from, "alerts@example.com");
        assert_eq!(targets[0].to, vec!["ops@example.com"]);
        assert_eq!(targets[0].cc, vec!["audit@example.com"]);
    }

    #[test]
    fn enabled_wecom_targets_extracts_enabled_robot_channels() {
        let channel_config = json!({
            "channels": [
                {
                    "type": "wecom",
                    "enabled": true,
                    "url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test",
                    "messageType": "text",
                    "mentionedList": ["@all"]
                },
                {
                    "type": "wecom",
                    "enabled": false,
                    "url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=disabled"
                }
            ]
        });

        let targets = enabled_wecom_targets(&channel_config);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].message_type, WecomMessageType::Text);
        assert_eq!(targets[0].mentioned_list, vec!["@all"]);
    }

    #[test]
    fn build_webhook_dispatches_embeds_rule_and_event_context() {
        let rule = sample_rule(json!({
            "channels": [
                {
                    "type": "webhook",
                    "enabled": true,
                    "url": "https://example.com/alerts",
                    "headers": {
                        "x-alert-token": "secret"
                    }
                }
            ]
        }));

        let dispatches = build_webhook_dispatches(&rule, &sample_event());

        assert_eq!(dispatches.len(), 1);
        assert_eq!(dispatches[0].url, "https://example.com/alerts");
        assert_eq!(
            dispatches[0]
                .headers
                .get("x-alert-token")
                .map(String::as_str),
            Some("secret")
        );
        assert_eq!(dispatches[0].payload["rule"]["id"], 7);
        assert_eq!(
            dispatches[0].payload["rule"]["ruleCode"],
            "relay_success_rate_low"
        );
        assert_eq!(dispatches[0].payload["event"]["eventCode"], "altevt_test");
        assert_eq!(
            dispatches[0].payload["event"]["sourceRef"],
            "2026-04-09|u:11|p:22|c:33|a:44|m:gpt-5.4"
        );
    }

    #[test]
    fn build_email_dispatches_builds_subject_and_body() {
        let rule = sample_rule(json!({
            "channels": [
                {
                    "type": "email",
                    "enabled": true,
                    "from": "alerts@example.com",
                    "to": ["ops@example.com"]
                }
            ]
        }));

        let dispatches = build_email_dispatches(&rule, &sample_event());

        assert_eq!(dispatches.len(), 1);
        assert_eq!(dispatches[0].from, "alerts@example.com");
        assert_eq!(dispatches[0].to, vec!["ops@example.com"]);
        assert!(dispatches[0].subject.contains("渠道成功率过低 触发告警"));
        assert!(
            dispatches[0]
                .text_body
                .contains("规则编码: relay_success_rate_low")
        );
        assert!(dispatches[0].text_body.contains("事件编码: altevt_test"));
    }

    #[test]
    fn build_wecom_dispatches_builds_robot_payload() {
        let rule = sample_rule(json!({
            "channels": [
                {
                    "type": "wecom",
                    "enabled": true,
                    "url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test",
                    "messageType": "text",
                    "mentionedMobileList": ["13800000000"]
                }
            ]
        }));

        let dispatches = build_wecom_dispatches(&rule, &sample_event());

        assert_eq!(dispatches.len(), 1);
        assert_eq!(dispatches[0].payload["msgtype"], "text");
        assert_eq!(
            dispatches[0].payload["text"]["mentioned_mobile_list"][0],
            "13800000000"
        );
        assert!(
            dispatches[0].payload["text"]["content"]
                .as_str()
                .expect("text content")
                .contains("事件编码: altevt_test")
        );
    }

    #[test]
    fn build_notification_dispatches_counts_all_supported_channels() {
        let rule = sample_rule(json!({
            "channels": [
                {
                    "type": "webhook",
                    "enabled": true,
                    "url": "https://example.com/alerts"
                },
                {
                    "type": "email",
                    "enabled": true,
                    "from": "alerts@example.com",
                    "to": ["ops@example.com"]
                },
                {
                    "type": "wecom",
                    "enabled": true,
                    "url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test"
                }
            ]
        }));

        let dispatches = build_notification_dispatches(&rule, &sample_event());

        assert_eq!(dispatches.len(), 3);
        assert!(matches!(dispatches[0], AlertDispatch::Webhook(_)));
        assert!(matches!(dispatches[1], AlertDispatch::Email(_)));
        assert!(matches!(dispatches[2], AlertDispatch::Wecom(_)));
    }

    #[test]
    fn build_notification_dispatches_returns_empty_when_no_supported_channel_exists() {
        let rule = sample_rule(json!({
            "channels": [
                {
                    "type": "sms",
                    "enabled": true,
                    "to": ["13800000000"]
                }
            ]
        }));

        let dispatches = build_notification_dispatches(&rule, &sample_event());

        assert!(dispatches.is_empty());
    }
}
