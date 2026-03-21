-- ============================================================
-- AI 会话表
-- 参考 llmgateway session / 应用层客户端会话归属
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.session (
    id                  BIGSERIAL       PRIMARY KEY,
    session_key         VARCHAR(64)     NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    service_account_id  BIGINT          NOT NULL DEFAULT 0,
    client_type         VARCHAR(32)     NOT NULL DEFAULT 'web',
    client_ip           VARCHAR(64)     NOT NULL DEFAULT '',
    user_agent          VARCHAR(512)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_active_at      TIMESTAMPTZ,
    expire_time         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_session_session_key ON ai.session (session_key);
CREATE INDEX idx_ai_session_user_last_active ON ai.session (user_id, last_active_at);
CREATE INDEX idx_ai_session_project_id ON ai.session (project_id);
CREATE INDEX idx_ai_session_token_id ON ai.session (token_id);
CREATE INDEX idx_ai_session_service_account_id ON ai.session (service_account_id);

COMMENT ON TABLE ai.session IS 'AI 会话表';
COMMENT ON COLUMN ai.session.id IS '会话ID';
COMMENT ON COLUMN ai.session.session_key IS '会话键';
COMMENT ON COLUMN ai.session.organization_id IS '组织ID';
COMMENT ON COLUMN ai.session.project_id IS '项目ID';
COMMENT ON COLUMN ai.session.user_id IS '用户ID';
COMMENT ON COLUMN ai.session.token_id IS '令牌ID';
COMMENT ON COLUMN ai.session.service_account_id IS '服务账号ID';
COMMENT ON COLUMN ai.session.client_type IS '客户端类型：web/app/sdk/agent';
COMMENT ON COLUMN ai.session.client_ip IS '客户端IP';
COMMENT ON COLUMN ai.session.user_agent IS '客户端UA';
COMMENT ON COLUMN ai.session.status IS '状态：1=活跃 2=过期 3=关闭';
COMMENT ON COLUMN ai.session.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.session.last_active_at IS '最后活跃时间';
COMMENT ON COLUMN ai.session.expire_time IS '过期时间';
COMMENT ON COLUMN ai.session.create_time IS '创建时间';
COMMENT ON COLUMN ai.session.update_time IS '更新时间';
