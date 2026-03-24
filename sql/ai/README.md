# AI Schema

更新时间：2026-03-21

这里是 `summer-ai-hub` 的 SQL source of truth。

命名规则：

- 文件名也去掉重复域前缀，例如 `request.sql`、`log.sql`、`channel.sql`
- 物理表统一落在 `ai` schema，且不再重复 `ai_` 前缀
- 保留字表名按 PostgreSQL 规则加引号：`ai."order"`、`ai."transaction"`

当前覆盖域：

- 多租户协作：`ai.organization`、`ai.team`、`ai.project`、`ai.org_membership`、`ai.team_membership`、`ai.project_membership`、`ai.invitation`、`ai.service_account`
- 企业身份：`ai.org_sso_config`、`ai.domain_verification`、`ai.sso_group_mapping`、`ai.org_scim_config`、`ai.scim_user_mapping`、`ai.scim_group_mapping`、`ai.rbac_policy`、`ai.rbac_policy_version`
- 路由与供应商：`ai.vendor`、`ai.channel`、`ai.channel_account`、`ai.ability`、`ai.routing_rule`、`ai.routing_target`
- 用户与额度：`ai.token`、`ai.user_quota`、`ai.group_ratio`、`ai.governance_budget`、`ai.governance_rate_limit`
- 请求链路：`ai.request`、`ai.request_execution`、`ai.log`、`ai.trace`、`ai.trace_span`
- 内容治理：`ai.guardrail_config`、`ai.guardrail_rule`、`ai.guardrail_violation`、`ai.guardrail_metric_daily`、`ai.prompt_protection_rule`
- 文件与 RAG：`ai.file`、`ai.managed_object`、`ai.vector_store`、`ai.vector_store_file`、`ai.data_storage`
- 价格与账务：`ai.model_config`、`ai.channel_model_price`、`ai.channel_model_price_version`、`ai.topup`、`ai.redemption`、`ai."order"`、`ai."transaction"`、`ai.discount`、`ai.referral`、`ai.payment_method`
- 订阅：`ai.subscription_plan`、`ai.user_subscription`、`ai.subscription_preconsume_record`
- 运维可靠性：`ai.channel_probe`、`ai.daily_stats`、`ai.idempotency_record`、`ai.usage_billing_dedup`、`ai.error_passthrough_rule`、`ai.dead_letter_queue`、`ai.retry_attempt`、`ai.scheduler_outbox`、`ai.alert_rule`、`ai.alert_event`、`ai.alert_silence`、`ai.usage_cleanup_task`
- 产品辅助：`ai.session`、`ai.thread`、`ai.conversation`、`ai.message`、`ai.task`、`ai.prompt_template`、`ai.config_entry`、`ai.plugin`、`ai.plugin_binding`、`ai.audit_log`

说明：

- 时间字段统一为 `TIMESTAMPTZ`
- JSON 类策略/映射字段统一为 `JSONB`
- 账号认证凭据层已经独立归到 `sys` 域，不放在 `ai` 域
- 老库迁移时，需要把原来的 `public.ai_*` 表移动到 `ai` schema 并去掉重复前缀
