-- ============================================================
-- AI 控制面审计日志表
-- 区分于 ai.log：这里记录后台/控制面的配置和权限变更
-- 参考 hadrian audit_logs / llmgateway audit_log
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.audit_log (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    team_id             BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    actor_type          VARCHAR(32)     NOT NULL DEFAULT 'user',
    actor_user_id       BIGINT          NOT NULL DEFAULT 0,
    service_account_id  BIGINT          NOT NULL DEFAULT 0,
    action              VARCHAR(64)     NOT NULL DEFAULT '',
    resource_type       VARCHAR(64)     NOT NULL DEFAULT '',
    resource_id         VARCHAR(128)    NOT NULL DEFAULT '',
    resource_name       VARCHAR(128)    NOT NULL DEFAULT '',
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    trace_id            VARCHAR(64)     NOT NULL DEFAULT '',
    ip_address          VARCHAR(64)     NOT NULL DEFAULT '',
    user_agent          VARCHAR(512)    NOT NULL DEFAULT '',
    change_set          JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_audit_log_org_create_time ON ai.audit_log (organization_id, create_time);
CREATE INDEX idx_ai_audit_log_actor_user_create_time ON ai.audit_log (actor_user_id, create_time);
CREATE INDEX idx_ai_audit_log_service_account_id ON ai.audit_log (service_account_id);
CREATE INDEX idx_ai_audit_log_action_create_time ON ai.audit_log (action, create_time);
CREATE INDEX idx_ai_audit_log_resource_type_id ON ai.audit_log (resource_type, resource_id);

COMMENT ON TABLE ai.audit_log IS 'AI 控制面审计日志表（配置/权限/运营动作审计）';
COMMENT ON COLUMN ai.audit_log.id IS '审计日志ID';
COMMENT ON COLUMN ai.audit_log.organization_id IS '组织ID（0=系统级）';
COMMENT ON COLUMN ai.audit_log.team_id IS '团队ID（0=无）';
COMMENT ON COLUMN ai.audit_log.project_id IS '项目ID（0=无）';
COMMENT ON COLUMN ai.audit_log.actor_type IS '操作者类型：user/service_account/system';
COMMENT ON COLUMN ai.audit_log.actor_user_id IS '操作者用户ID';
COMMENT ON COLUMN ai.audit_log.service_account_id IS '操作者服务账号ID';
COMMENT ON COLUMN ai.audit_log.action IS '动作编码，如 token.create/channel.update/member.remove';
COMMENT ON COLUMN ai.audit_log.resource_type IS '资源类型';
COMMENT ON COLUMN ai.audit_log.resource_id IS '资源ID';
COMMENT ON COLUMN ai.audit_log.resource_name IS '资源名称冗余';
COMMENT ON COLUMN ai.audit_log.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.audit_log.trace_id IS '链路追踪ID';
COMMENT ON COLUMN ai.audit_log.ip_address IS '客户端IP';
COMMENT ON COLUMN ai.audit_log.user_agent IS '客户端UA';
COMMENT ON COLUMN ai.audit_log.change_set IS '字段变更摘要（JSON）';
COMMENT ON COLUMN ai.audit_log.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.audit_log.status IS '结果状态：1=成功 2=拒绝 3=失败';
COMMENT ON COLUMN ai.audit_log.create_time IS '记录时间';
