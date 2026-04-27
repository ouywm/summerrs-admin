-- ============================================================
-- AI 请求主表
-- 记录面向客户端的一次完整请求
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.request (
    id                  BIGSERIAL       PRIMARY KEY,
    request_id          VARCHAR(64)     NOT NULL,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    conversation_id     BIGINT          NOT NULL DEFAULT 0,
    message_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    thread_id           BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    channel_group       VARCHAR(64)     NOT NULL DEFAULT 'default',
    source_type         VARCHAR(32)     NOT NULL DEFAULT 'api',
    endpoint            VARCHAR(64)     NOT NULL DEFAULT '/v1/chat/completions',
    request_format      VARCHAR(64)     NOT NULL DEFAULT 'openai/chat_completions',
    requested_model     VARCHAR(128)    NOT NULL DEFAULT '',
    upstream_model      VARCHAR(128)    NOT NULL DEFAULT '',
    is_stream           BOOLEAN         NOT NULL DEFAULT FALSE,
    client_ip           VARCHAR(64)     NOT NULL DEFAULT '',
    user_agent          VARCHAR(512)    NOT NULL DEFAULT '',
    request_headers     JSONB           NOT NULL DEFAULT '{}'::jsonb,
    request_body        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    response_body       JSONB,
    response_status_code INT            NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    error_message       TEXT            NOT NULL DEFAULT '',
    duration_ms         INT             NOT NULL DEFAULT 0,
    first_token_ms      INT             NOT NULL DEFAULT 0,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_request_request_id ON ai.request (request_id);
CREATE INDEX idx_ai_request_user_id_create_time ON ai.request (user_id, create_time);
CREATE INDEX idx_ai_request_token_id_create_time ON ai.request (token_id, create_time);
CREATE INDEX idx_ai_request_project_id_create_time ON ai.request (project_id, create_time);
CREATE INDEX idx_ai_request_conversation_id_create_time ON ai.request (conversation_id, create_time);
CREATE INDEX idx_ai_request_message_id_create_time ON ai.request (message_id, create_time);
CREATE INDEX idx_ai_request_session_id_create_time ON ai.request (session_id, create_time);
CREATE INDEX idx_ai_request_thread_id_create_time ON ai.request (thread_id, create_time);
CREATE INDEX idx_ai_request_trace_id_create_time ON ai.request (trace_id, create_time);
CREATE INDEX idx_ai_request_status_create_time ON ai.request (status, create_time);
CREATE INDEX idx_ai_request_requested_model ON ai.request (requested_model);

COMMENT ON TABLE ai.request IS 'AI 请求主表（客户端视角的一次完整请求）';
COMMENT ON COLUMN ai.request.id IS '请求主键';
COMMENT ON COLUMN ai.request.request_id IS '请求唯一标识';
COMMENT ON COLUMN ai.request.user_id IS '调用用户ID';
COMMENT ON COLUMN ai.request.token_id IS '调用令牌ID';
COMMENT ON COLUMN ai.request.project_id IS '所属项目ID（0 表示个人请求）';
COMMENT ON COLUMN ai.request.conversation_id IS '所属对话ID';
COMMENT ON COLUMN ai.request.message_id IS '触发本次请求的消息ID';
COMMENT ON COLUMN ai.request.session_id IS '所属会话ID';
COMMENT ON COLUMN ai.request.thread_id IS '所属线程ID';
COMMENT ON COLUMN ai.request.trace_id IS '所属追踪ID';
COMMENT ON COLUMN ai.request.channel_group IS '命中的用户/令牌分组';
COMMENT ON COLUMN ai.request.source_type IS '来源：api/playground/test/task 等';
COMMENT ON COLUMN ai.request.endpoint IS '请求 endpoint';
COMMENT ON COLUMN ai.request.request_format IS '外部协议格式';
COMMENT ON COLUMN ai.request.requested_model IS '客户端请求模型';
COMMENT ON COLUMN ai.request.upstream_model IS '最终映射后的上游模型';
COMMENT ON COLUMN ai.request.is_stream IS '是否流式';
COMMENT ON COLUMN ai.request.client_ip IS '客户端 IP';
COMMENT ON COLUMN ai.request.user_agent IS '客户端 UA';
COMMENT ON COLUMN ai.request.request_headers IS '请求头快照（脱敏后）';
COMMENT ON COLUMN ai.request.request_body IS '请求体快照';
COMMENT ON COLUMN ai.request.response_body IS '客户端最终收到的响应体（非流式或摘要）';
COMMENT ON COLUMN ai.request.response_status_code IS '返回给客户端的状态码';
COMMENT ON COLUMN ai.request.status IS '状态：1=待处理 2=处理中 3=成功 4=失败 5=取消';
COMMENT ON COLUMN ai.request.error_message IS '错误摘要';
COMMENT ON COLUMN ai.request.duration_ms IS '总耗时（毫秒）';
COMMENT ON COLUMN ai.request.first_token_ms IS '首 token 延迟（毫秒）';
COMMENT ON COLUMN ai.request.create_time IS '创建时间';
COMMENT ON COLUMN ai.request.update_time IS '更新时间';
